# v0.9.1 — relative-link sweep over the public Markdown

**Objective.** Every relative link in the Markdown that ships with v0.9.1
resolves from a clean clone, and no public page links to maintainer-local
material. This is a documentation repair, not a runtime change: no Rust file
was touched.

## Method

The checker is `scripts/evidence/v091-link-sweep/check-links.py` (maintainer
tree, local-only). It builds the release set from every path git knows about —
tracked plus untracked and not ignored — so pages created for this commit are
included and `.git/info/exclude` material is not. For each inline link it
resolves the target against the document's directory and classifies it:

- **broken** — the target is neither a file in the set nor a directory that
  contains one;
- **local-only** — the target resolves under `ROADMAP EQPROXY/`, `scripts/`,
  `report/`, `.veyra/`, `dist/`, or a root file the repository does not publish
  (`PARITY_MATRIX.md`, `USER_PAIN_POINTS.md`, `AGENTS.md`);
- **ok** — everything else.

Images are checked separately. Markdown section anchors are compared with the
GitHub-style slug of every heading (and explicit `<a name>`/`id`); `#L…` code
line anchors are not validated. The checker exits non-zero on any defect.

```console
$ python3 scripts/evidence/v091-link-sweep/check-links.py
```

## Before and after

| Measure | Before | After |
|---|---|---|
| Markdown files in the release set | 163 | 165 |
| Relative links | 1,184 | 1,144 |
| Broken | **81** | **0** |
| Pointing to local-only material | **40** | **0** |
| Relative images | 0 | 0 |
| Markdown section anchors checked | 10 | 11 |
| Missing anchors | 0 | 0 |

The after column includes this page, the checklist's citation of it and the
[v091-date-header](v091-date-header.md) page, which add two files and eight
links. The link count falls by 40 because 51 references
to material that is not published — ten references to raw `.log` files (nine
distinct files) ignored by `git`'s global `*.log` rule, the harnesses under
`scripts/`, the roadmap under `ROADMAP EQPROXY/`, the audit reports under
`report/`, the Veyra evidence under `.veyra/`, and `PARITY_MATRIX.md` — became
inline code paths instead of links. They still name the artefact in the
maintainer tree; a reader of the repository no longer gets a 404.

## Repairs

The 121 defects (81 broken + 40 local-only) were repaired as follows:

| Repair | Count | Example |
|---|---|---|
| Repo-root paths in `docs/evidence/` rewritten relative to the document | 65 | `eqproxy-net/src/admin.rs` → `../../eqproxy-net/src/admin.rs` |
| Wrong depth to `SPEC.md` | 1 | `../SPEC.md` → `../../SPEC.md` |
| Wrong depth to code from a nested evidence directory | 4 | `../../../eqproxy-net/...` → `../../eqproxy-net/...` |
| Local-only or ignored targets converted to code | 51 | a harness link under `scripts/` becomes `journey.py` plus a local-only code path |

The ADR set was also normalised to `../../SPEC.md`, and two references to a
non-existent ADR-0011 became [SPEC §7.5](../../SPEC.md) links. Those changes are
part of the same sweep and are cited in the [release checklist](../release-0.9.1.md).

The final pass after the `Date` evidence page was added reports
`files=165 links=1144 broken=0 local-only=0 images=0 broken=0 md-anchors=11
missing=0`, exit **0**.

## Limits

- The checker validates file targets and Markdown heading anchors, not rendered
  HTML or redirects; external URLs are out of scope.
- `#L…` line anchors are link fragments into source files, not headings, and are
  not validated by this checker.
- The ten ignored `.log` files still exist only in the maintainer tree. Turning
  them into links would require publishing them, which is a separate decision;
  this sweep makes the public documentation truthful without that decision.
- The set includes untracked new pages (`docs/semantics.md`,
  `docs/release-0.9.1.md`, this page, the 0.9.1 evidence) because they belong to
  the release commit. A checkout without them will not have those links, by
  design.
