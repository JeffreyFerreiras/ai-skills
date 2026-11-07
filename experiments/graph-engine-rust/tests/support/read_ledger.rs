use graph_engine::{
    application::{
        error::EngineResult,
        graph_ledger::{GraphLedgerReadPort, GraphRead},
    },
    compat::Value,
};
use std::{cell::RefCell, collections::VecDeque};

pub struct ReadStep {
    method: &'static str,
    query: GraphRead,
    parameters: Vec<Value>,
    rows: Vec<Value>,
}

impl ReadStep {
    pub fn first(query: GraphRead, parameters: Vec<Value>, row: Option<Value>) -> Self {
        Self {
            method: "first",
            query,
            parameters,
            rows: row.into_iter().collect(),
        }
    }
    pub fn all(query: GraphRead, parameters: Vec<Value>, rows: Vec<Value>) -> Self {
        Self {
            method: "all",
            query,
            parameters,
            rows,
        }
    }
}

pub struct ReadLedger(RefCell<VecDeque<ReadStep>>);

impl ReadLedger {
    pub fn new(steps: Vec<ReadStep>) -> Self {
        Self(RefCell::new(steps.into()))
    }
    pub fn finished(&self) {
        assert!(self.0.borrow().is_empty());
    }
    fn take(&self, method: &str, query: GraphRead, parameters: &[Value]) -> Vec<Value> {
        let step = self
            .0
            .borrow_mut()
            .pop_front()
            .expect("unexpected ledger query");
        assert_eq!(method, step.method);
        assert_eq!(
            std::mem::discriminant(&query),
            std::mem::discriminant(&step.query)
        );
        assert!(Value::List(parameters.to_vec()).container_eq(&Value::List(step.parameters)));
        step.rows
    }
}

impl GraphLedgerReadPort for ReadLedger {
    fn first(&self, query: GraphRead, parameters: &[Value]) -> EngineResult<Option<Value>> {
        Ok(self.take("first", query, parameters).into_iter().next())
    }
    fn all(&self, query: GraphRead, parameters: &[Value]) -> EngineResult<Vec<Value>> {
        Ok(self.take("all", query, parameters))
    }
    fn visit(
        &self,
        _: GraphRead,
        _: &[Value],
        _: &mut dyn FnMut(Value) -> EngineResult<()>,
    ) -> EngineResult<()> {
        panic!("unexpected streaming ledger query")
    }
}
