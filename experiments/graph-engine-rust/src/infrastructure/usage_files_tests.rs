#[allow(dead_code)]
#[path = "../../tests/support/usage_graph.rs"]
mod usage_graph;

use super::{
    read_prefix_with_open, EngineResult, Limits, Path, UsageBytes, UsageFault, UsageReadStage,
    UsageSourcePort,
};
use crate::application::usage_source;
use std::cell::RefCell;
use usage_graph::UsageGraph;

struct ObservedRead<F>(F);

impl<F> UsageSourcePort for ObservedRead<F>
where
    F: Fn(&Path, Limits) -> EngineResult<UsageBytes>,
{
    fn read_prefix(&self, path: &Path, limits: Limits) -> EngineResult<UsageBytes> {
        (self.0)(path, limits)
    }
}

#[test]
fn test_raced_nonregular_source_is_opened_nonblocking_and_rejected() {
    let fixture = UsageGraph::new();
    #[cfg(windows)]
    let nonblocking = 0x4000_0000;
    #[cfg(unix)]
    let nonblocking = libc::O_NONBLOCK;
    let observations = RefCell::new(Vec::new());
    let real_opens = RefCell::new(Vec::new());
    let observer = |stage, _: &Path, flags| {
        observations.borrow_mut().push((stage, flags));
        Ok(UsageFault {
            opened_nonregular: stage == UsageReadStage::Opened,
            ..UsageFault::default()
        })
    };
    let read = ObservedRead(|path: &Path, limits| {
        read_prefix_with_open(path, limits, &observer, nonblocking, |path, flags| {
            let supported = flags & !nonblocking;
            real_opens.borrow_mut().push((flags, supported));
            #[cfg(windows)]
            return super::snapshot_file::open_flags(path, supported);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                std::fs::OpenOptions::new()
                    .read(true)
                    .custom_flags(supported)
                    .open(path)
            }
        })
    });
    let source = usage_source::read_source(&read, &fixture.log, Limits::default()).unwrap();
    let observations = observations.into_inner();
    let requested: Vec<_> = observations
        .iter()
        .filter_map(|&(stage, flags)| (stage == UsageReadStage::BeforeOpen).then_some(flags))
        .collect();
    assert_eq!(requested.len(), 1);
    assert_ne!(requested[0] & nonblocking, 0);
    assert_eq!(
        real_opens.into_inner(),
        [(requested[0], requested[0] & !nonblocking)]
    );
    assert!(observations.contains(&(UsageReadStage::Opened, requested[0])));
    assert!(!observations
        .iter()
        .any(|(stage, _)| *stage == UsageReadStage::BeforeRead));
    assert!(source.source_id.is_none());
    assert_eq!(source.diagnostics, [(0, "source_changed".to_owned())]);
    fixture.tear_down();
}
