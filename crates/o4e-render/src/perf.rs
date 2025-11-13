// this_file: crates/o4e-render/src/perf.rs

//! Performance optimization utilities for o4e rendering.

use parking_lot::RwLock;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Performance metrics collector
pub struct PerfMetrics {
    render_times: RwLock<VecDeque<Duration>>,
    shape_times: RwLock<VecDeque<Duration>>,
    cache_hits: AtomicUsize,
    cache_misses: AtomicUsize,
    total_renders: AtomicUsize,
    start_time: Instant,
    max_samples: usize,
}

impl PerfMetrics {
    pub fn new(max_samples: usize) -> Self {
        Self {
            render_times: RwLock::new(VecDeque::with_capacity(max_samples)),
            shape_times: RwLock::new(VecDeque::with_capacity(max_samples)),
            cache_hits: AtomicUsize::new(0),
            cache_misses: AtomicUsize::new(0),
            total_renders: AtomicUsize::new(0),
            start_time: Instant::now(),
            max_samples,
        }
    }

    pub fn record_render(&self, duration: Duration) {
        let mut times = self.render_times.write();
        if times.len() >= self.max_samples {
            times.pop_front();
        }
        times.push_back(duration);
        self.total_renders.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_shape(&self, duration: Duration) {
        let mut times = self.shape_times.write();
        if times.len() >= self.max_samples {
            times.pop_front();
        }
        times.push_back(duration);
    }

    pub fn record_cache_hit(&self) {
        self.cache_hits.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_cache_miss(&self) {
        self.cache_misses.fetch_add(1, Ordering::Relaxed);
    }

    pub fn get_stats(&self) -> PerfStats {
        let render_times = self.render_times.read();
        let shape_times = self.shape_times.read();

        let avg_render = if !render_times.is_empty() {
            let sum: Duration = render_times.iter().sum();
            sum / render_times.len() as u32
        } else {
            Duration::ZERO
        };

        let avg_shape = if !shape_times.is_empty() {
            let sum: Duration = shape_times.iter().sum();
            sum / shape_times.len() as u32
        } else {
            Duration::ZERO
        };

        let cache_hits = self.cache_hits.load(Ordering::Relaxed);
        let cache_misses = self.cache_misses.load(Ordering::Relaxed);
        let total = cache_hits + cache_misses;
        let cache_hit_rate = if total > 0 {
            (cache_hits as f64) / (total as f64)
        } else {
            0.0
        };

        let total_renders = self.total_renders.load(Ordering::Relaxed);
        let uptime = self.start_time.elapsed();
        let renders_per_second = if uptime.as_secs() > 0 {
            (total_renders as f64) / uptime.as_secs_f64()
        } else {
            0.0
        };

        PerfStats {
            avg_render_time: avg_render,
            avg_shape_time: avg_shape,
            cache_hit_rate,
            renders_per_second,
            total_renders,
            uptime,
        }
    }

    pub fn reset(&self) {
        self.render_times.write().clear();
        self.shape_times.write().clear();
        self.cache_hits.store(0, Ordering::Relaxed);
        self.cache_misses.store(0, Ordering::Relaxed);
        self.total_renders.store(0, Ordering::Relaxed);
    }
}

#[derive(Debug, Clone)]
pub struct PerfStats {
    pub avg_render_time: Duration,
    pub avg_shape_time: Duration,
    pub cache_hit_rate: f64,
    pub renders_per_second: f64,
    pub total_renders: usize,
    pub uptime: Duration,
}

/// Buffer pool for reusing allocations
pub struct BufferPool {
    pools: Arc<RwLock<Vec<Vec<u8>>>>,
    max_size: usize,
}

impl BufferPool {
    pub fn new(max_size: usize) -> Self {
        Self {
            pools: Arc::new(RwLock::new(Vec::with_capacity(max_size))),
            max_size,
        }
    }

    pub fn get(&self, capacity: usize) -> PooledBuffer {
        let mut pools = self.pools.write();

        // Try to find a buffer with sufficient capacity
        if let Some(index) = pools.iter().position(|b| b.capacity() >= capacity) {
            let buffer = pools.swap_remove(index);
            return PooledBuffer {
                buffer,
                pool: Arc::downgrade(&self.pools),
            };
        }

        // Create new buffer if none available
        PooledBuffer {
            buffer: Vec::with_capacity(capacity),
            pool: Arc::downgrade(&self.pools),
        }
    }

    pub fn clear(&self) {
        self.pools.write().clear();
    }
}

pub struct PooledBuffer {
    buffer: Vec<u8>,
    pool: std::sync::Weak<RwLock<Vec<Vec<u8>>>>,
}

impl PooledBuffer {
    pub fn as_mut(&mut self) -> &mut Vec<u8> {
        &mut self.buffer
    }

    pub fn into_inner(mut self) -> Vec<u8> {
        // Prevent Drop from returning the buffer to the pool
        self.pool = std::sync::Weak::new();
        std::mem::take(&mut self.buffer)
    }
}

impl Drop for PooledBuffer {
    fn drop(&mut self) {
        if let Some(pool) = self.pool.upgrade() {
            let mut pools = pool.write();
            if pools.len() < 64 {
                // Limit pool size
                self.buffer.clear();
                pools.push(std::mem::take(&mut self.buffer));
            }
        }
    }
}

/// Optimized memory operations
pub mod mem_ops {
    use std::arch::x86_64::*;

    /// SIMD-accelerated BGRA to RGBA conversion
    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "sse2")]
    pub unsafe fn bgra_to_rgba_simd(data: &mut [u8]) {
        if !is_x86_feature_detected!("sse2") {
            bgra_to_rgba_fallback(data);
            return;
        }

        let len = data.len();
        let simd_len = len - (len % 16);

        // Process 16 bytes (4 pixels) at a time with SIMD
        for i in (0..simd_len).step_by(16) {
            let ptr = data.as_mut_ptr().add(i);

            // Load 16 bytes
            let pixels = _mm_loadu_si128(ptr as *const __m128i);

            // Shuffle mask to swap B and R channels: BGRA -> RGBA
            let shuffle = _mm_setr_epi8(2, 1, 0, 3, 6, 5, 4, 7, 10, 9, 8, 11, 14, 13, 12, 15);
            let swapped = _mm_shuffle_epi8(pixels, shuffle);

            // Store back
            _mm_storeu_si128(ptr as *mut __m128i, swapped);
        }

        // Handle remaining bytes
        for i in (simd_len..len).step_by(4) {
            data.swap(i, i + 2);
        }
    }

    #[cfg(not(target_arch = "x86_64"))]
    pub fn bgra_to_rgba_simd(data: &mut [u8]) {
        bgra_to_rgba_fallback(data);
    }

    fn bgra_to_rgba_fallback(data: &mut [u8]) {
        for chunk in data.chunks_exact_mut(4) {
            chunk.swap(0, 2);
        }
    }

    /// Fast memory clear
    pub fn fast_clear(data: &mut [u8], value: u8) {
        unsafe {
            std::ptr::write_bytes(data.as_mut_ptr(), value, data.len());
        }
    }

    /// Fast memory copy
    pub fn fast_copy(dst: &mut [u8], src: &[u8]) {
        assert_eq!(dst.len(), src.len());
        unsafe {
            std::ptr::copy_nonoverlapping(src.as_ptr(), dst.as_mut_ptr(), src.len());
        }
    }
}

/// Profiling scope guard
pub struct PerfScope<'a> {
    metrics: &'a PerfMetrics,
    start: Instant,
    metric_type: MetricType,
}

pub enum MetricType {
    Render,
    Shape,
}

impl<'a> PerfScope<'a> {
    pub fn new(metrics: &'a PerfMetrics, metric_type: MetricType) -> Self {
        Self {
            metrics,
            start: Instant::now(),
            metric_type,
        }
    }
}

impl<'a> Drop for PerfScope<'a> {
    fn drop(&mut self) {
        let duration = self.start.elapsed();
        match self.metric_type {
            MetricType::Render => self.metrics.record_render(duration),
            MetricType::Shape => self.metrics.record_shape(duration),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_perf_metrics() {
        let metrics = PerfMetrics::new(100);

        // Record some samples
        metrics.record_render(Duration::from_millis(1));
        metrics.record_render(Duration::from_millis(2));
        metrics.record_shape(Duration::from_millis(3));
        metrics.record_cache_hit();
        metrics.record_cache_miss();

        let stats = metrics.get_stats();
        assert_eq!(stats.total_renders, 2);
        assert_eq!(stats.cache_hit_rate, 0.5);
    }

    #[test]
    fn test_buffer_pool() {
        let pool = BufferPool::new(10);

        {
            let mut buffer1 = pool.get(1024);
            buffer1.as_mut().resize(1024, 0);
        } // buffer1 returned to pool

        {
            let buffer2 = pool.get(512);
            // Should reuse buffer1 since it has capacity >= 512
            assert!(buffer2.buffer.capacity() >= 1024);
        }
    }

    #[test]
    fn test_bgra_to_rgba() {
        let mut data = vec![0, 1, 2, 3, 4, 5, 6, 7]; // BGRA pixels
        unsafe {
            mem_ops::bgra_to_rgba_simd(&mut data);
        }
        assert_eq!(data, vec![2, 1, 0, 3, 6, 5, 4, 7]); // RGBA pixels
    }
}
