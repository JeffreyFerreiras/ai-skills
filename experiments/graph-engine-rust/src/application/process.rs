use super::error::EngineResult;
use crate::compat::PyString;
use std::{path::Path, time::Duration};

pub struct ProcessRequest<'a> {
    pub arguments: &'a [PyString],
    pub directory: &'a Path,
    pub timeout: Duration,
    pub stdout_limit: Option<usize>,
    pub capture_stderr: bool,
}

#[derive(Debug)]
pub enum ProcessOutcome {
    Completed {
        exit_code: i64,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
    },
    TimedOut {
        stdout: Vec<u8>,
        stderr: Vec<u8>,
    },
    OutputLimit,
}

pub trait ProcessPort {
    fn capture(&self, request: &ProcessRequest<'_>) -> EngineResult<ProcessOutcome>;
}
