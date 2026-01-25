---
description: Triage a GitHub issue by label and create bd tasks
---

You are an opencode command. Use the GitHub issue number $1.

1. Assign the issue to yourself:
   - gh issue edit $1 --add-assignee "@me"
2. Fetch issue metadata with gh:
   - gh issue view $1 --json title,body,labels,url,number
3. Normalize label names to lowercase and classify the issue type:
   - Bug if any label contains "bug" or "type: bug".
   - Enhancement/feature if any label contains "enhancement", "feature", or "feature request".
   - If both appear, treat as bug.
   - If neither appears, summarize the labels and ask the user to classify.

Bug flow:
- Analyze the codebase relevant to the issue description (search, read files, and trace the flow).
- Summarize findings and likely root cause areas.
- Create one or more bd tasks that include:
  - Findings summary (what you observed in code).
  - Goals (what must be fixed or clarified).
  - Validation plan (tests or checks to run).
- Use: bd create "<task title>" --type task --priority 2 --external-ref "gh-$1" --description "Findings: ...\n\nGoals: ..." --acceptance "Validation: ..."

Enhancement/feature flow:
- Update product docs to reflect the change:
  - docs/prd.md
  - docs/technical-design.md (if design impact)
  - docs/acceptance-tests.md (if acceptance criteria change)
- Draft a concise implementation plan (steps + dependencies).
- Create bd tasks that include:
  - Goals (what to build/change).
  - Plan (key steps).
  - Validation plan (tests or checks to run).
- Use: bd create "<task title>" --type task --priority 2 --external-ref "gh-$1" --description "Goals: ...\n\nPlan: ..." --acceptance "Validation: ..."

Always call out which validations you ran or plan to run.
