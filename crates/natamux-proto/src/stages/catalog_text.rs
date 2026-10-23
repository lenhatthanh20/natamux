//! The phase, sentence, default prompt and `needsCommand` of each stage of `STAGE_CATALOG`
//! (`packages/core/src/stages/stage-catalog.ts`), in its order. `tests/core_values_stages.rs`
//! checks every text against the TypeScript.

use crate::stages::catalog::StagePhase;

pub(crate) struct Text {
    #[cfg(test)]
    pub id: &'static str,
    pub phase: StagePhase,
    pub description: &'static str,
    pub prompt: Option<&'static str>,
    pub needs_command: bool,
}

pub(crate) const TEXTS: [Text; 28] = [
    Text {
        #[cfg(test)]
        id: "context",
        phase: StagePhase::Understand,
        description: "Reads the context sources of the task into context.md.",
        prompt: None,
        needs_command: false,
    },
    Text {
        #[cfg(test)]
        id: "research",
        phase: StagePhase::Understand,
        description: "Reads the code and docs the task touches, and lists what it found.",
        prompt: Some(concat!(
            "Task: {title}\n",
            "Read {dir}/context.md. Then read the code and the docs that this task touches.\n",
            "Write {dir}/research.md with these sections: Related files, How it works today, Risks.\n",
            "Do not change any code.",
        )),
        needs_command: false,
    },
    Text {
        #[cfg(test)]
        id: "clarify",
        phase: StagePhase::Understand,
        description: "Lists what is unclear in the task, with a suggested answer for each.",
        prompt: Some(concat!(
            "Task: {title}\n",
            "Read {dir}/context.md.\n",
            "Write {dir}/questions.md with a Questions section: each thing that is unclear in the task, with a suggested answer.\n",
            "Do not change any code.",
        )),
        needs_command: false,
    },
    Text {
        #[cfg(test)]
        id: "spec",
        phase: StagePhase::Design,
        description: "Writes the goal, the scope, the acceptance criteria and a map of the code to change.",
        prompt: Some(concat!(
            "Task: {title}\n",
            "Read {dir}/context.md. Write {dir}/spec.md with these sections: Goal, Scope, Out of scope, Acceptance criteria, Code map.\n",
            "In Code map, list each file and function the change touches, with one line on why.\n",
            "Do not change any code.",
        )),
        needs_command: false,
    },
    Text {
        #[cfg(test)]
        id: "ux-design",
        phase: StagePhase::Design,
        description: "Describes the screens, their states and flows, and keyboard and accessibility.",
        prompt: Some(concat!(
            "Task: {title}\n",
            "Read {dir}/context.md and {dir}/spec.md when it is there.\n",
            "Write {dir}/ux.md with these sections: Screens, States (empty, loading, error), Flows, Accessibility.\n",
            "Do not change any code.",
        )),
        needs_command: false,
    },
    Text {
        #[cfg(test)]
        id: "architecture",
        phase: StagePhase::Design,
        description: "Describes the components, the data flow, the interfaces and the trade-offs.",
        prompt: Some(concat!(
            "Task: {title}\n",
            "Read {dir}/context.md and {dir}/spec.md when it is there, then the code it touches.\n",
            "Write {dir}/architecture.md with these sections: Components, Data flow, Interfaces, Trade-offs.\n",
            "Do not change any code.",
        )),
        needs_command: false,
    },
    Text {
        #[cfg(test)]
        id: "threat-model",
        phase: StagePhase::Design,
        description: "Lists what an attacker could reach, and how each threat is handled.",
        prompt: Some(concat!(
            "Task: {title}\n",
            "Read {dir}/context.md and {dir}/spec.md when it is there, then the code it touches.\n",
            "Write {dir}/threat-model.md with these sections: Assets, Entry points, Threats (each with how it is handled).\n",
            "Do not change any code.",
        )),
        needs_command: false,
    },
    Text {
        #[cfg(test)]
        id: "plan",
        phase: StagePhase::Design,
        description: "Writes the steps, the files to change and the tests to add.",
        prompt: Some(concat!(
            "Task: {title}\n",
            "Read {dir}/context.md and {dir}/spec.md. Write {dir}/plan.md with these sections: Steps, Files, Tests.\n",
            "Do not change any code.",
        )),
        needs_command: false,
    },
    Text {
        #[cfg(test)]
        id: "test-spec",
        phase: StagePhase::Build,
        description: "Writes test cases from the spec, without reading the code.",
        prompt: Some(concat!(
            "Task: {title}\n",
            "Read {dir}/context.md and {dir}/spec.md. Do not read the code.\n",
            "Write {dir}/test-spec.md with a Cases section: one case per item, each with its input, the expected result and the edge cases.\n",
            "Do not change any code.",
        )),
        needs_command: false,
    },
    Text {
        #[cfg(test)]
        id: "implement",
        phase: StagePhase::Build,
        description: "Writes the tests first, then the code that makes them pass.",
        prompt: Some(concat!(
            "Task: {title}\n",
            "Read {dir}/spec.md and {dir}/plan.md when they are there.\n",
            "Write the tests first, then the code that makes them pass. {commit}",
        )),
        needs_command: false,
    },
    Text {
        #[cfg(test)]
        id: "migration",
        phase: StagePhase::Build,
        description: "Writes the data or schema migration, and how to roll it back.",
        prompt: Some(concat!(
            "Task: {title}\n",
            "Read {dir}/spec.md and {dir}/plan.md when they are there.\n",
            "Write the data or schema migration this task needs, with a way to roll it back, and a test for both. {commit}",
        )),
        needs_command: false,
    },
    Text {
        #[cfg(test)]
        id: "refactor",
        phase: StagePhase::Build,
        description: "Cleans up the changed code without changing what it does.",
        prompt: Some(concat!(
            "Task: {title}\n",
            "Look at the changes of this task. Clean them up: names, duplicate code, long functions.\n",
            "Do not change what the code does. The tests must still pass. {commit}",
        )),
        needs_command: false,
    },
    Text {
        #[cfg(test)]
        id: "docs",
        phase: StagePhase::Build,
        description: "Updates the user and developer docs for the change.",
        prompt: Some(concat!(
            "Task: {title}\n",
            "Look at the changes of this task. Update the user docs and the developer docs that they make out of date. {commit}",
        )),
        needs_command: false,
    },
    Text {
        #[cfg(test)]
        id: "i18n",
        phase: StagePhase::Build,
        description: "Adds or updates the translations of new UI text.",
        prompt: Some(concat!(
            "Task: {title}\n",
            "Look at the changes of this task. Add or update the translations of each new or changed UI text. {commit}",
        )),
        needs_command: false,
    },
    Text {
        #[cfg(test)]
        id: "lint",
        phase: StagePhase::Check,
        description: "Runs the project's lint and type check command.",
        prompt: None,
        needs_command: true,
    },
    Text {
        #[cfg(test)]
        id: "self-review",
        phase: StagePhase::Check,
        description: "Reads the diff against the spec, lists the problems and fixes them.",
        prompt: Some(concat!(
            "Task: {title}\n",
            "Read {dir}/spec.md when it is there. Read the diff of this task against it.\n",
            "Write {dir}/self-review.md with a Problems section, then fix each problem. {commit}",
        )),
        needs_command: false,
    },
    Text {
        #[cfg(test)]
        id: "security",
        phase: StagePhase::Check,
        description: "Checks the diff for security problems.",
        prompt: Some(concat!(
            "Task: {title}\n",
            "Read the diff of this task. Look for security problems: input that is not checked, secrets, injection, access checks.\n",
            "Write {dir}/security.md with a Findings section: each problem, where it is, and how to fix it.\n",
            "Do not change any code.",
        )),
        needs_command: false,
    },
    Text {
        #[cfg(test)]
        id: "performance",
        phase: StagePhase::Check,
        description: "Looks for slow paths in the diff, and measures them when it can.",
        prompt: Some(concat!(
            "Task: {title}\n",
            "Read the diff of this task. Look for slow paths: loops over large data, extra requests, work done twice. Measure when you can.\n",
            "Write {dir}/performance.md with a Findings section.\n",
            "Do not change any code.",
        )),
        needs_command: false,
    },
    Text {
        #[cfg(test)]
        id: "accessibility",
        phase: StagePhase::Check,
        description: "Checks UI changes for keyboard, screen reader and contrast problems.",
        prompt: Some(concat!(
            "Task: {title}\n",
            "Read the UI changes of this task. Check keyboard use, screen reader names and roles, focus, and color contrast.\n",
            "Write {dir}/accessibility.md with a Findings section.\n",
            "Do not change any code.",
        )),
        needs_command: false,
    },
    Text {
        #[cfg(test)]
        id: "agent-review",
        phase: StagePhase::Check,
        description: "An agent reviews the changes against the spec, and sends the task back to Implement when they need fixes.",
        prompt: Some(concat!(
            "Task: {title}\n",
            "Read {dir}/spec.md, and {dir}/test-spec.md when it is there. Review the changes of this task against them: what is not committed (git status, git diff) and the commits of this branch that are not on the default branch (git log, and git diff from the merge base). Read the changed files. Do not change any code.\n",
            "Write {dir}/review.md with a Findings section: each problem that must be fixed, with its file and why, or \"None\".\n",
            "When a problem must be fixed, run `natamux stage back $NATAMUX_TASK_ID --reason \"<the problems, one per line>\"` and stop. Do not run natamux done then.",
        )),
        needs_command: false,
    },
    Text {
        #[cfg(test)]
        id: "review",
        phase: StagePhase::Check,
        description: "A person reads the diff, comments, and approves it or sends it back.",
        prompt: None,
        needs_command: false,
    },
    Text {
        #[cfg(test)]
        id: "testing",
        phase: StagePhase::Check,
        description: "An agent adds a test for each case of the test spec and runs all tests, or a command runs them.",
        prompt: Some(concat!(
            "Task: {title}\n",
            "For each case in {dir}/test-spec.md that has no test yet, add one. Then run all the tests and fix what fails. {commit}",
        )),
        needs_command: false,
    },
    Text {
        #[cfg(test)]
        id: "e2e",
        phase: StagePhase::Check,
        description: "Adds end-to-end tests for the change, then runs them.",
        prompt: Some(concat!(
            "Task: {title}\n",
            "Add end-to-end tests for what this task changes, where the project has end-to-end tests. {commit}",
        )),
        needs_command: true,
    },
    Text {
        #[cfg(test)]
        id: "qa",
        phase: StagePhase::Check,
        description: "A person tries the change by hand.",
        prompt: None,
        needs_command: false,
    },
    Text {
        #[cfg(test)]
        id: "design-review",
        phase: StagePhase::Check,
        description: "A designer checks the UI.",
        prompt: None,
        needs_command: false,
    },
    Text {
        #[cfg(test)]
        id: "changelog",
        phase: StagePhase::Ship,
        description: "Adds a changelog entry for the change.",
        prompt: Some(concat!(
            "Task: {title}\n",
            "Look at the changes of this task. Add an entry for them to the changelog of the project. {commit}",
        )),
        needs_command: false,
    },
    Text {
        #[cfg(test)]
        id: "staging",
        phase: StagePhase::Ship,
        description: "You check the change on a staging server.",
        prompt: None,
        needs_command: false,
    },
    Text {
        #[cfg(test)]
        id: "release",
        phase: StagePhase::Ship,
        description: "Pushes the branch and opens a pull request. Ends when it is merged.",
        prompt: None,
        needs_command: false,
    },
];
