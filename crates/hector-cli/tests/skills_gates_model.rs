//! Drift guard: the shipped Claude Code skills must teach the 0.3 **gates**
//! model, never the retired pre-0.3 engine/severity/rules model.
//!
//! These SKILL.md files are prose the agent loads to interpret hook output and
//! to author `.hector.yml`. If they describe `engine:`/`severity:`/`{file}`
//! templating or a `violations`/`rule_id` verdict, they hand the agent a schema
//! that no longer exists — a silent correctness bug that no other test catches
//! because the files aren't compiled or embedded. This scan fails the build if
//! any retired vocabulary creeps back in, and asserts the gates-model anchors
//! are present.

use std::path::PathBuf;

fn skills_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../adapters/claude-code/skills")
}

fn read_skill(name: &str) -> String {
    let path = skills_dir().join(name).join("SKILL.md");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

const SKILLS: &[&str] = &["hector", "hector-author", "hector-init", "hector-review"];

/// Vocabulary that only existed in the pre-0.3 engine model. A gate is exactly
/// `{ files, run }`; there are no engines, no severity, no capability sandbox,
/// no `{file}` templating, and the verdict uses `blocks`, not `violations`.
const RETIRED_TOKENS: &[&str] = &[
    "engine:",        // gates have no engine field
    "severity",       // no severity tier (and no warn)
    "rule_id",        // verdict/telemetry key by `gate`, not `rule_id`
    "passed_checks",  // verdict field is `passed`
    "violations",     // verdict field is `blocks`
    "{file}",         // path arrives as $HECTOR_FILE; no templating
    "capabilities:",  // no sandbox in 0.3 — timeout is the only rail
    "hector migrate", // no migration path exists
];

#[test]
fn skills_contain_no_retired_engine_model_vocabulary() {
    for skill in SKILLS {
        let body = read_skill(skill);
        for token in RETIRED_TOKENS {
            assert!(
                !body.contains(token),
                "{skill}/SKILL.md still teaches the retired model: contains `{token}`"
            );
        }
    }
}

#[test]
fn runtime_skill_describes_the_gates_verdict_shape() {
    // The `hector` skill interprets the block verdict the agent sees on stderr.
    // That JSON is the schema-4 verdict: `blocks[]` of `{gate,file,message}`.
    let body = read_skill("hector");
    assert!(
        body.contains("blocks"),
        "hector/SKILL.md must describe the `blocks` verdict array"
    );
    assert!(
        body.contains("\"gate\""),
        "hector/SKILL.md must key a block by `gate`"
    );
}

#[test]
fn author_skill_teaches_the_two_field_gate() {
    // Authoring must describe a gate as `{files, run}` blocking on exit 2, with
    // the path as $HECTOR_FILE — not engines/severity/{file}.
    let body = read_skill("hector-author");
    for anchor in ["$HECTOR_FILE", "run:", "files:", "exit 2"] {
        assert!(
            body.contains(anchor),
            "hector-author/SKILL.md must teach the gates model: missing `{anchor}`"
        );
    }
}
