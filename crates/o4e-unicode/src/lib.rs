// this_file: crates/o4e-unicode/src/lib.rs

//! Unicode-aware text segmentation utilities shared across backends.

use icu_properties::{
    maps::{self, CodePointMapDataBorrowed},
    names::PropertyEnumToValueNameLinearMapperBorrowed,
    Script,
};
use icu_segmenter::{GraphemeClusterSegmenter, WordSegmenter};
use o4e_core::{
    types::{Direction, SegmentOptions, TextRun},
    Result,
};
use unicode_bidi::BidiInfo;

/// Unicode-aware segmenter that powers all o4e backends.
pub struct TextSegmenter {
    script_map: CodePointMapDataBorrowed<'static, Script>,
    script_name_mapper: PropertyEnumToValueNameLinearMapperBorrowed<'static, Script>,
}

impl TextSegmenter {
    /// Create a new segmenter with ICU data baked in.
    pub fn new() -> Self {
        Self {
            script_map: maps::script(),
            script_name_mapper: Script::enum_to_long_name_mapper(),
        }
    }

    /// Segment text into runs that respect grapheme clusters, bidi, script, and optional word chunks.
    pub fn segment(&self, text: &str, options: &SegmentOptions) -> Result<Vec<TextRun>> {
        if text.is_empty() {
            return Ok(Vec::new());
        }

        let grapheme_boundaries: Vec<usize> =
            GraphemeClusterSegmenter::new().segment_str(text).collect();
        if grapheme_boundaries.len() < 2 {
            return Ok(vec![self.build_run(
                text,
                0,
                text.len(),
                Script::Common,
                &options.language.clone().unwrap_or_else(|| "en".to_string()),
                Direction::LeftToRight,
            )]);
        }

        let cluster_spans: Vec<(usize, usize)> = grapheme_boundaries
            .windows(2)
            .map(|pair| (pair[0], pair[1]))
            .collect();

        let line_breaks = Self::hard_line_breaks(text);
        let word_breaks: Vec<usize> = if options.font_fallback {
            WordSegmenter::new_auto().segment_str(text).collect()
        } else {
            Vec::new()
        };

        let language = options.language.clone().unwrap_or_else(|| "en".to_string());
        let slices = self.compute_bidi_slices(text, options.bidi_resolve);
        let mut runs = Vec::with_capacity(slices.len());

        for slice in slices {
            self.collect_runs_in_slice(
                text,
                slice,
                &cluster_spans,
                &line_breaks,
                if options.font_fallback {
                    Some(&word_breaks)
                } else {
                    None
                },
                options,
                &language,
                &mut runs,
            );
        }

        if runs.is_empty() {
            runs.push(self.build_run(
                text,
                0,
                text.len(),
                Script::Common,
                &language,
                Direction::LeftToRight,
            ));
        }

        Ok(runs)
    }

    fn compute_bidi_slices(&self, text: &str, resolve: bool) -> Vec<TextSlice> {
        if text.is_empty() {
            return Vec::new();
        }

        if !resolve {
            return vec![TextSlice {
                start: 0,
                end: text.len(),
                direction: Direction::LeftToRight,
            }];
        }

        let bidi = BidiInfo::new(text, None);
        let mut slices = Vec::new();

        for paragraph in &bidi.paragraphs {
            let line = paragraph.range.clone();
            let (levels, runs_vec) = bidi.visual_runs(paragraph, line.clone());

            for run in runs_vec {
                if run.start >= run.end {
                    continue;
                }
                let absolute_start = line.start + run.start;
                let absolute_end = line.start + run.end;
                let level = levels.get(run.start).copied().unwrap_or(paragraph.level);
                let direction = if level.is_rtl() {
                    Direction::RightToLeft
                } else {
                    Direction::LeftToRight
                };

                slices.push(TextSlice {
                    start: absolute_start,
                    end: absolute_end,
                    direction,
                });
            }
        }

        if slices.is_empty() {
            slices.push(TextSlice {
                start: 0,
                end: text.len(),
                direction: Direction::LeftToRight,
            });
        }

        slices
    }

    #[allow(clippy::too_many_arguments)]
    fn collect_runs_in_slice(
        &self,
        text: &str,
        slice: TextSlice,
        cluster_spans: &[(usize, usize)],
        line_breaks: &[usize],
        word_breaks: Option<&[usize]>,
        options: &SegmentOptions,
        language: &str,
        runs: &mut Vec<TextRun>,
    ) {
        if slice.end <= slice.start {
            return;
        }

        let slice_line_breaks: Vec<usize> = line_breaks
            .iter()
            .copied()
            .filter(|idx| *idx > slice.start && *idx < slice.end)
            .collect();
        let mut line_cursor = 0usize;

        let slice_word_breaks: Vec<usize> = word_breaks
            .unwrap_or(&[])
            .iter()
            .copied()
            .filter(|idx| *idx > slice.start && *idx < slice.end)
            .collect();
        let mut word_cursor = 0usize;

        let mut run_start = slice.start;
        let mut current_script: Option<Script> = None;

        for &(cluster_start, cluster_end) in cluster_spans {
            if cluster_end <= slice.start {
                continue;
            }
            if cluster_start >= slice.end {
                break;
            }

            let start = cluster_start.max(slice.start);
            let end = cluster_end.min(slice.end);
            if start >= end {
                continue;
            }

            let cluster_script = self.detect_script(&text[start..end]);
            let script_changed = options.script_itemize
                && self.is_significant_script(cluster_script)
                && current_script
                    .map(|existing| existing != cluster_script)
                    .unwrap_or(false);

            if script_changed && start > run_start {
                let script_for_run = current_script.unwrap_or(cluster_script);
                runs.push(self.build_run(
                    text,
                    run_start,
                    start,
                    script_for_run,
                    language,
                    slice.direction,
                ));
                run_start = start;
                current_script = None;
            }

            if current_script.is_none() && self.is_significant_script(cluster_script) {
                current_script = Some(cluster_script);
            }

            let mut boundary_hit = Self::hit_boundary(&slice_line_breaks, &mut line_cursor, end);
            if !boundary_hit && options.font_fallback && !slice_word_breaks.is_empty() {
                boundary_hit = Self::hit_boundary(&slice_word_breaks, &mut word_cursor, end);
            }

            if boundary_hit {
                if end > run_start {
                    let script_for_run = current_script.unwrap_or(cluster_script);
                    runs.push(self.build_run(
                        text,
                        run_start,
                        end,
                        script_for_run,
                        language,
                        slice.direction,
                    ));
                }
                run_start = end;
                current_script = None;
            }
        }

        if run_start < slice.end {
            let script_for_run =
                current_script.unwrap_or_else(|| self.detect_script(&text[run_start..slice.end]));
            runs.push(self.build_run(
                text,
                run_start,
                slice.end,
                script_for_run,
                language,
                slice.direction,
            ));
        }
    }

    fn detect_script(&self, fragment: &str) -> Script {
        for ch in fragment.chars() {
            let script = self.script_map.get(ch);
            if self.is_significant_script(script) {
                return script;
            }
        }
        Script::Common
    }

    fn build_run(
        &self,
        text: &str,
        start: usize,
        end: usize,
        script: Script,
        language: &str,
        direction: Direction,
    ) -> TextRun {
        TextRun {
            text: text[start..end].to_string(),
            range: (start, end),
            script: self.script_label(script),
            language: language.to_string(),
            direction,
            font: None,
        }
    }

    fn script_label(&self, script: Script) -> String {
        self.script_name_mapper
            .get(script)
            .unwrap_or("Unknown")
            .to_string()
    }

    fn is_significant_script(&self, script: Script) -> bool {
        !matches!(script, Script::Common | Script::Inherited | Script::Unknown)
    }

    fn hit_boundary(boundaries: &[usize], cursor: &mut usize, position: usize) -> bool {
        while *cursor < boundaries.len() && boundaries[*cursor] < position {
            *cursor += 1;
        }

        if *cursor < boundaries.len() && boundaries[*cursor] == position {
            *cursor += 1;
            return true;
        }

        false
    }

    fn hard_line_breaks(text: &str) -> Vec<usize> {
        let mut breaks: Vec<usize> = text.match_indices('\n').map(|(idx, _)| idx + 1).collect();
        breaks.extend(
            text.match_indices('\r')
                .filter(|(idx, _)| text.as_bytes().get(idx + 1) != Some(&b'\n'))
                .map(|(idx, _)| idx + 1),
        );
        breaks.sort_unstable();
        breaks
    }
}

impl Default for TextSegmenter {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy)]
struct TextSlice {
    start: usize,
    end: usize,
    direction: Direction,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segment(text: &str, mut options: SegmentOptions) -> Vec<TextRun> {
        let segmenter = TextSegmenter::new();
        options.language = Some("en".to_string());
        segmenter.segment(text, &options).unwrap()
    }

    #[test]
    fn segment_simple_latin_text() {
        let runs = segment("Hello World", SegmentOptions::default());
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].script, "Latin");
        assert_eq!(runs[0].direction, Direction::LeftToRight);
    }

    #[test]
    fn segment_bidi_text_into_runs() {
        let mut options = SegmentOptions::default();
        options.script_itemize = true;
        options.bidi_resolve = true;
        let runs = segment("Hello مرحبا", options);
        assert!(runs.len() >= 2);
        let last = runs.last().unwrap();
        assert_eq!(runs[0].script, "Latin");
        assert_eq!(runs[0].direction, Direction::LeftToRight);
        assert_eq!(last.script, "Arabic");
        assert_eq!(last.direction, Direction::RightToLeft);
    }

    #[test]
    fn segment_respects_line_breaks() {
        let runs = segment("Line1\nLine2", SegmentOptions::default());
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].text, "Line1\n");
        assert_eq!(runs[1].text, "Line2");
    }

    #[test]
    fn segment_splits_on_word_boundaries_for_fallback() {
        let mut options = SegmentOptions::default();
        options.font_fallback = true;
        let runs = segment("Word One", options);
        assert!(runs.len() >= 2);
    }

    #[test]
    fn segment_itemizes_cjk_and_latin() {
        let mut options = SegmentOptions::default();
        options.script_itemize = true;
        let runs = segment("漢字ABC", options);
        assert!(runs.len() >= 2);
        assert_eq!(runs[0].script, "Han");
        assert_eq!(runs.last().unwrap().script, "Latin");
    }
}
