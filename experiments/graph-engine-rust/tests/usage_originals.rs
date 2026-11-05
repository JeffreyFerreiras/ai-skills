#[path = "../src/suite/usage_originals.rs"]
mod usage_originals;

#[test]
fn exact_original_usage_cases() {
    for (name, run) in usage_originals::CASES {
        eprintln!("original usage case {name}");
        run();
    }
}
