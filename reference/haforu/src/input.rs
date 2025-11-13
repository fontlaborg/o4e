// this_file: src/input.rs

//! Helpers for parsing job input payloads for the CLI.

use anyhow::{anyhow, bail, Result};

use haforu::batch::{Job, JobSpec};
use haforu::security::MAX_JOBS_PER_SPEC;

/// Parse stdin payload into a list of jobs.
///
/// Accepts either a full `JobSpec` JSON blob or newline-delimited `Job` objects.
pub fn parse_jobs_payload(payload: &str) -> Result<Vec<Job>> {
    let trimmed = payload.trim();
    if trimmed.is_empty() {
        bail!("No jobs supplied in stdin payload");
    }

    if trimmed.starts_with('{') {
        if let Ok(spec) = serde_json::from_str::<JobSpec>(trimmed) {
            spec.validate()?;
            return Ok(spec.jobs);
        }
    }

    let mut jobs = Vec::new();
    for (idx, line) in payload.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let job: Job = serde_json::from_str(line)
            .map_err(|e| anyhow!("Line {}: invalid JSON: {}", idx + 1, e))?;
        job.validate()?;
        jobs.push(job);
        if jobs.len() > MAX_JOBS_PER_SPEC {
            bail!("Too many jobs ({}), max {}", jobs.len(), MAX_JOBS_PER_SPEC);
        }
    }

    if jobs.is_empty() {
        bail!("No jobs parsed from JSONL input");
    }

    Ok(jobs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_spec_payload() {
        let json = r#"{
            "version": "1.0",
            "jobs": [
                {
                    "id": "spec",
                    "font": {"path": "/tmp/font.ttf", "size": 1000, "variations": {}},
                    "text": {"content": "a"},
                    "rendering": {"format": "pgm", "encoding": "base64", "width": 10, "height": 10}
                }
            ]
        }"#;
        let jobs = parse_jobs_payload(json).expect("spec parse ok");
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].id, "spec");
    }

    #[test]
    fn parse_jsonl_payload() {
        let jsonl = r#"{"id":"a","font":{"path":"/tmp/font.ttf","size":1000,"variations":{}},"text":{"content":"a"},"rendering":{"format":"pgm","encoding":"base64","width":10,"height":10}}
{"id":"b","font":{"path":"/tmp/font.ttf","size":1000,"variations":{}},"text":{"content":"b"},"rendering":{"format":"pgm","encoding":"base64","width":10,"height":10}}"#;
        let jobs = parse_jobs_payload(jsonl).expect("jsonl parse ok");
        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[0].id, "a");
        assert_eq!(jobs[1].id, "b");
    }

    #[test]
    fn reject_empty_payload() {
        let err = parse_jobs_payload(" \n\t").unwrap_err();
        assert!(err.to_string().contains("No jobs"));
    }
}
