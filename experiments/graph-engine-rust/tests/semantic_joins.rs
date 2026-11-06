use graph_engine::{
    application::{
        error::{EngineError, EngineResult},
        graph_ledger::{GraphLedgerReadPort, GraphRead},
        join_validation::canonical_collection_members,
        semantic_joins::validate_joins,
    },
    compat::{self, SerializationProfile, Value},
    domain::ids,
};

struct Fixture {
    run: Value,
    policy: Value,
    plan: Value,
    join: Value,
    members: Vec<Value>,
    registry_ref: Option<Value>,
}

impl Fixture {
    fn new(key: &str, kind: &str, stage: &str, names: &[&str]) -> Self {
        let digest = "a".repeat(64);
        let join_id = ids::stable_id("R", &digest, "join", key, 0, None).unwrap();
        Self {
            run: Value::object([
                ("run_id", "R".into()),
                ("policy_digest", digest.into()),
                ("selected_route", "advisory".into()),
                ("selected_tags_json", "[]".into()),
            ]),
            policy: Value::object([("specialists", Value::object([]))]),
            plan: Value::object([("plan_json", "{}".into())]),
            join: Value::object([
                ("join_id", join_id.into()),
                ("join_key", key.into()),
                ("kind", kind.into()),
                ("stage", stage.into()),
                ("generation", 0_i64.into()),
                ("status", "open".into()),
                ("result_json", Value::Null),
            ]),
            members: names
                .iter()
                .map(|name| {
                    Value::object([
                        ("branch_id", format!("branch-{name}").into()),
                        ("node_key", (*name).into()),
                        ("join_mandatory", 1_i64.into()),
                        ("generation", 0_i64.into()),
                        ("status", "pending".into()),
                        ("result_json", Value::Null),
                        ("result_digest", Value::Null),
                        ("failure_code", Value::Null),
                        ("reason_code", Value::Null),
                    ])
                })
                .collect(),
            registry_ref: None,
        }
    }

    fn verify(&self) -> EngineResult<()> {
        validate_joins(self, &self.run, &self.policy)
    }

    fn expect(&self, code: &str) {
        match self.verify() {
            Err(EngineError::State(error)) => assert_eq!(error.code, code),
            result => panic!("expected {code}, got {result:?}"),
        }
    }
}

impl GraphLedgerReadPort for Fixture {
    fn first(&self, query: GraphRead, parameters: &[Value]) -> EngineResult<Option<Value>> {
        Ok(match query {
            GraphRead::ExecutionPlanBody => Some(self.plan.clone()),
            GraphRead::Artifact
                if self
                    .registry_ref
                    .as_ref()
                    .is_some_and(|reference| reference.py_eq(&parameters[0])) =>
            {
                Some(Value::object([]))
            }
            _ => None,
        })
    }

    fn all(&self, query: GraphRead, _: &[Value]) -> EngineResult<Vec<Value>> {
        Ok(match query {
            GraphRead::JoinsForValidation => vec![self.join.clone()],
            GraphRead::JoinMembers => self.members.clone(),
            _ => panic!("unexpected query {query:?}"),
        })
    }

    fn visit(
        &self,
        query: GraphRead,
        parameters: &[Value],
        visit: &mut dyn FnMut(Value) -> EngineResult<()>,
    ) -> EngineResult<()> {
        for value in self.all(query, parameters)? {
            visit(value)?;
        }
        Ok(())
    }
}

#[test]
fn join_binding_id_and_member_generation_fail_in_source_order() {
    let mut fixture = Fixture::new("closure", "closure", "closure", &["advisory_reviewer"]);
    fixture.verify().unwrap();
    fixture.join.set("kind", "dependency".into());
    fixture.join.set("join_id", "wrong".into());
    fixture.expect("JOIN_STATE_INVALID");
    fixture.join.set("kind", "closure".into());
    fixture.expect("STABLE_ID_INVALID");
    fixture.join.set(
        "join_id",
        ids::stable_id("R", &"a".repeat(64), "join", "closure", 0, None)
            .unwrap()
            .into(),
    );
    fixture.members[0].set("generation", 1_i64.into());
    fixture.expect("JOIN_MEMBERSHIP_INVALID");
    fixture.members[0].set("generation", 0_i64.into());
    fixture.members[0].set("join_mandatory", 0_i64.into());
    fixture.expect("JOIN_MEMBERSHIP_INVALID");
}

#[test]
fn collection_requires_exact_frozen_members_and_its_canonical_registry_digest() {
    let mut fixture = Fixture::new(
        "research_collection",
        "collection",
        "research",
        &["design_research_architecture", "design_research_validation"],
    );
    let frozen = canonical_collection_members(&fixture, &fixture.members).unwrap();
    fixture.join.set("status", "sealed".into());
    fixture.join.set(
        "result_json",
        String::from_utf8(compat::dumps(&frozen, SerializationProfile::CLI).unwrap())
            .unwrap()
            .into(),
    );
    fixture.expect("COLLECTION_ARTIFACT_INVALID");
    let wrapper = Value::object([
        ("schema_version", 1_i64.into()),
        ("kind", "collection".into()),
        ("join_id", fixture.join["join_id"].clone()),
        ("members", frozen),
    ]);
    fixture.registry_ref = Some(
        format!(
            "ledger:{}#sha256={}",
            fixture.join["join_id"].text().unwrap(),
            ids::sha256_bytes(&ids::canonical_bytes(&wrapper).unwrap())
        )
        .into(),
    );
    fixture.verify().unwrap();
    fixture.members.reverse();
    fixture.expect("COLLECTION_ARTIFACT_INVALID");
    fixture.join.set("result_json", "{".into());
    fixture.expect("JOIN_RESULT_INVALID");
    fixture.join.set("result_json", Value::Null);
    fixture.expect("JOIN_RESULT_INVALID");
}

#[test]
fn senior_redesign_exception_keeps_its_single_member_and_generation_constraints() {
    let mut fixture = Fixture::new(
        "delivery_collection",
        "collection",
        "delivery",
        &["senior_engineer"],
    );
    fixture.members[0].set("status", "succeeded".into());
    fixture.members[0].set("result_json", "{\"decision\":\"REDESIGN_REQUIRED\"}".into());
    fixture.verify().unwrap();
    fixture.members[0].set("generation", 1_i64.into());
    fixture.expect("JOIN_MEMBERSHIP_INVALID");
    fixture.members[0].set("generation", 0_i64.into());
    fixture.members[0].set("result_json", "{".into());
    fixture.expect("JOIN_MEMBERSHIP_INVALID");
}

#[test]
fn delivery_panel_and_selected_specialist_are_members_of_the_same_bound_join() {
    let names = [
        "code_reviewer",
        "test_engineer",
        "code_reviewer_architecture",
        "code_reviewer_naming",
        "code_reviewer_bug_hunter",
        "security_reviewer",
    ];
    let mut fixture = Fixture::new("delivery_collection", "collection", "delivery", &names);
    fixture
        .plan
        .set("plan_json", "{\"code_review_panel_version\":1}".into());
    fixture
        .run
        .set("selected_tags_json", "[\"security\"]".into());
    fixture.policy.set(
        "specialists",
        Value::object([(
            "security",
            Value::object([("node_key", "security_reviewer".into())]),
        )]),
    );
    fixture.verify().unwrap();
    fixture.members.pop();
    fixture.expect("JOIN_MEMBERSHIP_INVALID");
}
