# v0.9.2 operator-task study — protocol and status

Status: **NOT EXECUTED. No participants have been recruited; no sessions,
timings or questionnaire responses exist. This directory is a protocol, not
evidence of UX.**

Casilla 14 has two halves: the static-Envoy/HAProxy benchmark (published in
[`../v092-bench-envoy/README.md`](../v092-bench-envoy/README.md)) and this
operator-task study. The complete casilla was moved to v0.9.4 on 2026-11-05;
the benchmark is executed, but the study remains open until real participants
complete it. The task definitions come from
[`BENCHMARK_PLAN.md`](../../../BENCHMARK_PLAN.md) §7 (T-01…T-10).

## 1. Question and arms

The exploratory question is: **how do operators perform and what do they
experience when they carry out common proxy tasks in EQProxy versus HAProxy
3.4.6?** The study is not a market survey and does not generalize beyond the
recruited participants.

- **Arm A:** the EQProxy candidate binary and its published documentation.
- **Arm B:** HAProxy 3.4.6 and its published documentation.
- Static Envoy is **not** a third arm in this protocol. It is measured in the
  benchmark only; adding it here would require a separate decision before any
  recruitment. No Envoy operator data exists.

Use the same host, task materials, browser/terminal, network access and time
limits for both arms. Avoid internal EQProxy shortcuts that an external
operator would not have.

## 2. Participants and design

- Target **8 completed participants**; retain at least **5 complete paired
  sessions** before analysing. This is a small exploratory sample, not an
  inferential one.
- Recruit both **novice** and **experienced** operators. Record prior HAProxy,
  nginx and Envoy experience, but do not preselect only experts.
- Use a **within-subject, counterbalanced AB/BA** design: half start with
  EQProxy, half with HAProxy. Record the order and any carry-over.
- Obtain informed consent before starting. Participation is voluntary and can
  stop at any time. Do not record names, emails or signatures in this
  repository.
- Each session is capped at **90 minutes**; each task has a **10-minute**
  working cap unless the participant stops earlier.

## 3. Tasks

The task names are fixed by `BENCHMARK_PLAN.md` §7. The observable completion
criteria below make the scoring reproducible without prescribing one exact
configuration path.

| ID | Task | Observable completion criterion |
|---|---|---|
| T-01 | Create a basic proxy | A listener accepts a request and returns the origin response; participant states it is working. |
| T-02 | Add a backend | The new backend receives traffic through the proxy without breaking the existing path. |
| T-03 | Configure TLS | A client completes a TLS request to the proxy with the provided certificate; no private key is copied to the wrong path. |
| T-04 | Diagnose a 502/503 | Participant identifies the cause in the supplied broken setup and restores a successful response. |
| T-05 | Identify a timeout | Participant names the concrete timeout responsible for the supplied symptom and points to its setting. |
| T-06 | Apply a change | Change is applied without an unintended restart/dataplane outage; participant confirms the new behaviour. |
| T-07 | Revert the change | The previous working behaviour is restored and confirmed. |
| T-08 | Drain a server | New traffic stops going to the target while the in-flight request is allowed to finish; participant confirms the state. |
| T-09 | Detect a wrong rule | Participant identifies the wrong rule, explains the observed request, and corrects it using the available diagnostics. |
| T-10 | Migrate a reference-compatible config | Participant produces a working EQProxy configuration from the supplied HAProxy-shaped input and lists unsupported directives. |

Do not silently help with a task. If a participant asks for help, record the
request; the task is not counted as an unaided success.

## 4. Measures and procedure

For each task and arm record:

- **time to completion** (seconds, from task start to the participant's
  confirmation or the cap);
- **success** (unaided success, success after a hint, failure, not attempted);
- **errors** (invalid command/config, wrong file, destructive retry, extra
  restart);
- **documentation lookups** (count and which page);
- **perceived clarity** (1 = very unclear … 5 = very clear);
- **perceived risk of the change** (1 = none … 5 = very high);
- short factual notes and, with separate permission, anonymized quotations.

Procedure per participant:

1. Read the information sheet and sign consent outside this repository.
2. Record intake and prior experience; assign AB or BA order.
3. Give the same short orientation to both arms; do not teach the tasks.
4. Run T-01…T-10 in the assigned arm, then the other arm, with the same
   materials and caps.
5. Apply the clarity/risk questionnaire after each arm.
6. Debrief: biggest friction, what they would need before migrating, and any
   task they would not attempt unaided.
7. Store the pseudonymous session record; keep consent and any recordings
   outside the repository.

## 5. Analysis and reporting

- Report every task, including failures and tasks stopped at the cap; do not
  drop an unfavourable task.
- Report per-participant times, medians and min–max across participants; show
  paired differences (EQProxy − HAProxy) rather than only an aggregate.
- Treat the sample as exploratory: no p-values or “users prefer” market claim
  from five to eight participants.
- Separate measured facts (time, success, errors) from impressions
  (clarity/risk, quotations). State the arm order and any learning effect.
- Publish the blank protocol/forms and the anonymized aggregate; do not publish
  consent forms, names or raw recordings.

## 6. Status and next actions

- [ ] Recruit real participants with the declared experience mix.
- [ ] Run and record paired sessions with the forms in [`forms.md`](forms.md).
- [ ] Analyze every task and publish the anonymized aggregate.
- [ ] Only then decide whether casilla 14 can close.

Until those steps are complete, the correct status is **NOT EXECUTED**, not
“passed”, “simulated” or “representative”. The benchmark in
[`../v092-bench-envoy/`](../v092-bench-envoy/) does not substitute for this
study.
