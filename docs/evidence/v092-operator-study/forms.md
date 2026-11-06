# v0.9.2 operator-task study — blank forms

These forms are **templates only**. They contain no participant data. Do not
fill them with fictional participants, and do not commit consent forms,
names, emails, signatures, recordings or raw quotations without permission.
Use a pseudonymous ID such as `P-001`; keep the linkage key outside this
repository.

## 1. Participant intake

| Field | Value |
|---|---|
| Participant ID | `<P-001>` |
| Consent version/date | `<version / YYYY-MM-DD>` |
| Experience band | `<novice / intermediate / expert>` |
| Years operating reverse proxies | `<number>` |
| Prior HAProxy experience | `<none / some / regular>` |
| Prior nginx experience | `<none / some / regular>` |
| Prior Envoy experience | `<none / some / regular>` |
| Accessibility or scheduling notes | `<text; no medical detail unless volunteered>` |
| Assigned order | `<AB / BA>` |
| Session date (UTC) | `<YYYY-MM-DD>` |

## 2. Session log

One row per task and arm. Use `—` for not applicable; never leave a failed task
out.

### Arm A — EQProxy

| Task | Start (UTC) | End (UTC) | Time s | Success (unaided / hint / failed / not attempted) | Errors | Doc lookups | Clarity 1–5 | Risk 1–5 | Notes |
|---|---|---|---:|---|---|---:|---:|---:|---|
| T-01 |  |  |  |  |  |  |  |  |  |
| T-02 |  |  |  |  |  |  |  |  |  |
| T-03 |  |  |  |  |  |  |  |  |  |
| T-04 |  |  |  |  |  |  |  |  |  |
| T-05 |  |  |  |  |  |  |  |  |  |
| T-06 |  |  |  |  |  |  |  |  |  |
| T-07 |  |  |  |  |  |  |  |  |  |
| T-08 |  |  |  |  |  |  |  |  |  |
| T-09 |  |  |  |  |  |  |  |  |  |
| T-10 |  |  |  |  |  |  |  |  |  |

### Arm B — HAProxy 3.4.6

| Task | Start (UTC) | End (UTC) | Time s | Success (unaided / hint / failed / not attempted) | Errors | Doc lookups | Clarity 1–5 | Risk 1–5 | Notes |
|---|---|---|---:|---|---|---:|---:|---:|---|
| T-01 |  |  |  |  |  |  |  |  |  |
| T-02 |  |  |  |  |  |  |  |  |  |
| T-03 |  |  |  |  |  |  |  |  |  |
| T-04 |  |  |  |  |  |  |  |  |  |
| T-05 |  |  |  |  |  |  |  |  |  |
| T-06 |  |  |  |  |  |  |  |  |  |
| T-07 |  |  |  |  |  |  |  |  |  |
| T-08 |  |  |  |  |  |  |  |  |  |
| T-09 |  |  |  |  |  |  |  |  |  |
| T-10 |  |  |  |  |  |  |  |  |  |

## 3. Post-arm questionnaire

Repeat for each arm.

| Question | 1 | 2 | 3 | 4 | 5 |
|---|---|---|---|---|---|
| The task goal was clear. |  |  |  |  |  |
| The documentation helped me recover from an error. |  |  |  |  |  |
| I understood whether my change was active. |  |  |  |  |  |
| I would feel safe applying this change in production. |  |  |  |  |  |
| The diagnostics told me what to fix. |  |  |  |  |  |

Free text: `<what was the single biggest source of friction?>`

## 4. Debrief

- Which task would you not attempt unaided in production?
- Which error message or output was most useful, and which was misleading?
- What would you need before migrating an existing HAProxy configuration?
- Did the arm order affect what you learned or how you approached the second
  arm?
- Permission for an anonymized quotation: `<yes / no / not requested>`

## 5. Analysis sheet

One row per task, after all participants have completed the study.

| Task | Arm | n complete | Median time s | Min–max s | Unaided success | Failed / capped | Median clarity | Median risk |
|---|---|---:|---:|---:|---:|---:|---:|---:|
| T-01 | EQProxy |  |  |  |  |  |  |  |
| T-01 | HAProxy |  |  |  |  |  |  |  |
| T-02 | EQProxy |  |  |  |  |  |  |  |
| T-02 | HAProxy |  |  |  |  |  |  |  |
| T-03 | EQProxy |  |  |  |  |  |  |  |
| T-03 | HAProxy |  |  |  |  |  |  |  |
| T-04 | EQProxy |  |  |  |  |  |  |  |
| T-04 | HAProxy |  |  |  |  |  |  |  |
| T-05 | EQProxy |  |  |  |  |  |  |  |
| T-05 | HAProxy |  |  |  |  |  |  |  |
| T-06 | EQProxy |  |  |  |  |  |  |  |
| T-06 | HAProxy |  |  |  |  |  |  |  |
| T-07 | EQProxy |  |  |  |  |  |  |  |
| T-07 | HAProxy |  |  |  |  |  |  |  |
| T-08 | EQProxy |  |  |  |  |  |  |  |
| T-08 | HAProxy |  |  |  |  |  |  |  |
| T-09 | EQProxy |  |  |  |  |  |  |  |
| T-09 | HAProxy |  |  |  |  |  |  |  |
| T-10 | EQProxy |  |  |  |  |  |  |  |
| T-10 | HAProxy |  |  |  |  |  |  |  |

Add a paired-difference column and notes when analysing; do not hide a task
because its result is unfavourable to EQProxy.
