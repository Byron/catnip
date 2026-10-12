# Repository instructions

## Commits

- Create a commit for each completed standalone change after its relevant checks
  pass, without waiting for a separate request to commit. Keep unrelated changes
  in separate commits.
- Use Conventional Commits for the subject: `type(scope): description`, with an
  optional scope (for example, `feat(cli): make launch settings discoverable`).
- Write a descriptive commit body in Markdown. Focus on the motivation: the
  problem, why the change is useful, and any meaningful tradeoffs or constraints.
  Avoid restating implementation details already apparent from the diff.
- Include relevant validation results and any checks that were skipped. Scale
  the detail to the change; a small change needs only a short explanation.
