# Visual elevator pitch

IronLint turns "please follow our rules" into an automatic feedback loop for AI coding agents. Good edits keep moving. Risky edits get stopped or surfaced while the agent can still fix them.

```mermaid
flowchart LR
    subgraph Without["Without IronLint"]
        Speed["AI writes code fast"]
        Drift["Rules live in memory, docs, and review comments"]
        Late["Problems show up late<br/>security, tests, architecture, style"]
        Rework["Humans spend review time<br/>catching preventable issues"]

        Speed --> Drift --> Late --> Rework
    end

    subgraph With["With IronLint"]
        Intent["Team standards<br/>what good looks like"]
        Gate["IronLint policy gate<br/>always beside the agent"]
        Proposed["AI proposes an edit"]
        Check{"Does this fit<br/>the repo's rules?"}
        Clean["Clean edit lands<br/>work keeps flowing"]
        Block["Block edit<br/>agent rewrites before it moves on"]
        Signal["Telemetry<br/>see noisy, valuable, and dead checks"]
        Bridge["Late cleanup becomes<br/>live guidance"]

        Intent --> Gate
        Proposed --> Gate
        Gate --> Check
        Check -->|yes| Clean
        Check -->|no| Block
        Block --> Proposed
        Check --> Signal
        Signal --> Intent
    end

    Rework -.-> Bridge
    Bridge -.-> Gate
```

## The pitch

- **For teams:** IronLint makes standards enforceable at the moment code is written, not after the review queue is already full.
- **For agents:** IronLint gives precise feedback, so the agent can correct itself instead of guessing what "good" means in this repo.
- **For reviewers:** IronLint absorbs the repetitive policy checks, leaving humans more room for design, product judgment, and taste.
- **For operators:** IronLint leaves a trail, so teams can see which checks are helping, which are noisy, and which need tightening.

## One sentence

IronLint is a seatbelt for AI coding: it lets agents move quickly while keeping the work inside the rules your team actually cares about.
