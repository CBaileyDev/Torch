//! Stage prompt templates. Every prompt carries the original goal verbatim
//! plus the accumulated artifact, so no stage ever sees only the previous
//! stage's output. Users can override any template from the GUI; overrides
//! are overlaid onto these defaults by the shell.

use serde::{Deserialize, Serialize};

/// Substitute `{{key}}` placeholders. Unknown placeholders are left intact
/// so a typo in a user-edited template is visible instead of silent.
pub fn render(template: &str, vars: &[(&str, &str)]) -> String {
    let mut out = template.to_string();
    for (key, value) in vars {
        out = out.replace(&format!("{{{{{key}}}}}"), value);
    }
    out
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Templates {
    pub intake_questions: String,
    pub intake_brief: String,
    pub plan: String,
    pub critic: String,
    pub merge: String,
    pub implement: String,
    pub refine: String,
    // Classic Linear preset
    pub architect: String,
    pub drafter: String,
    pub reviser: String,
}

impl Default for Templates {
    fn default() -> Self {
        Self {
            intake_questions: INTAKE_QUESTIONS.to_string(),
            intake_brief: INTAKE_BRIEF.to_string(),
            plan: PLAN.to_string(),
            critic: CRITIC.to_string(),
            merge: MERGE.to_string(),
            implement: IMPLEMENT.to_string(),
            refine: REFINE.to_string(),
            architect: ARCHITECT.to_string(),
            drafter: DRAFTER.to_string(),
            reviser: REVISER.to_string(),
        }
    }
}

const INTAKE_QUESTIONS: &str = "\
You are the intake stage of a multi-stage build pipeline. Operate at {{effort}} effort.

GOAL (verbatim from the user):
{{goal}}

Ask the 3-5 sharpest clarifying questions whose answers would most change the
implementation. Output ONLY the questions, one per line, numbered `1.` to `5.`,
each ending with a question mark. Do not answer them yourself.";

const INTAKE_BRIEF: &str = "\
You are the intake stage of a multi-stage build pipeline. Operate at {{effort}} effort.

GOAL (verbatim from the user):
{{goal}}

The user answered your clarifying questions:
{{answers}}

Write two things:
1. A concise build brief capturing the goal, the constraints from the answers,
   and explicit non-goals.
2. The shell commands that deterministically verify the result (build, test,
   lint). Output each one on its own line prefixed exactly with `VERIFY: `.";

const PLAN: &str = "\
You are the planner of a multi-stage build pipeline. Operate at {{effort}} effort.

GOAL (verbatim from the user):
{{goal}}

ACCUMULATED ARTIFACT (brief so far):
{{artifact}}

Produce the full implementation spec: stack and key dependencies, module
breakdown with responsibilities, public contracts between modules, data
shapes, and ordered milestones. If the goal touches fast-moving dependencies,
research current versions before pinning them. Be precise enough that a
fresh session could implement from the spec alone.";

const CRITIC: &str = "\
You are an adversarial critic reviewing an implementation plan you did not
write. Operate at {{effort}} effort. You share no context with the planner —
attack the plan on the merits.

GOAL (verbatim from the user):
{{goal}}

PLAN UNDER REVIEW:
{{artifact}}

List concrete findings: ordering problems, missing error handling, unstated
assumptions, contract gaps, testing blind spots. Number each finding `F1.`,
`F2.`, … with a one-line actionable fix. No praise, no summary.";

const MERGE: &str = "\
You are the merge stage of a multi-stage build pipeline. Operate at {{effort}} effort.

GOAL (verbatim from the user):
{{goal}}

CURRENT PLAN:
{{artifact}}

CRITIC FINDINGS:
{{critiques}}

Fold every accepted finding into a single revised spec. For any finding you
reject, say why in one line. Output the complete revised spec — this is the
document the implementer will work from.";

const IMPLEMENT: &str = "\
You are the implementer of a multi-stage build pipeline. Operate at {{effort}} effort.
You are running inside the user's working directory: write real files.

GOAL (verbatim from the user):
{{goal}}

SPEC:
{{artifact}}

Implement the spec. Create and edit files directly. Follow the spec's module
layout and contracts exactly; where the spec is silent, choose the simplest
thing that could work. Do not claim success — verification runs separately.";

const REFINE: &str = "\
You are the refiner of a multi-stage build pipeline. Operate at {{effort}} effort.
You are resuming the implementation session in the user's working directory.

GOAL (verbatim from the user):
{{goal}}

The orchestrator ran the verify commands. They failed:
{{failures}}

Fix the failures by editing files directly. Address the root cause, not the
symptom. Do not claim success — verification runs again after you finish.";

const ARCHITECT: &str = "\
You are the architect (Classic Linear preset). Operate at {{effort}} effort.

GOAL (verbatim from the user):
{{goal}}

Define the system architecture: components, boundaries, data flow, and the
key technical decisions with one-line rationale each.";

const DRAFTER: &str = "\
You are the drafter (Classic Linear preset). Operate at {{effort}} effort.

GOAL (verbatim from the user):
{{goal}}

UPSTREAM ARTIFACT:
{{artifact}}

Draft the implementation plan from the artifact above: files to create,
ordered work items, and the verification commands.";

const REVISER: &str = "\
You are the reviser (Classic Linear preset). Operate at {{effort}} effort.

GOAL (verbatim from the user):
{{goal}}

DRAFT:
{{artifact}}

Revise the draft: fix inconsistencies, fill gaps, and tighten it into the
final document the implementer will follow.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_substitutes_known_and_keeps_unknown() {
        let out = render("a {{x}} b {{missing}}", &[("x", "1")]);
        assert_eq!(out, "a 1 b {{missing}}");
    }

    #[test]
    fn default_templates_have_goal_placeholder() {
        let templates = Templates::default();
        for (name, body) in [
            ("intake_questions", &templates.intake_questions),
            ("intake_brief", &templates.intake_brief),
            ("plan", &templates.plan),
            ("critic", &templates.critic),
            ("merge", &templates.merge),
            ("implement", &templates.implement),
            ("refine", &templates.refine),
            ("architect", &templates.architect),
            ("drafter", &templates.drafter),
            ("reviser", &templates.reviser),
        ] {
            assert!(body.contains("{{goal}}"), "{name} is missing {{{{goal}}}}");
        }
    }
}
