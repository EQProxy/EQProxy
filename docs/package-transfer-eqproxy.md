# Migrating the GHCR package to the EQProxy account

Language: **English** (public documentation).

Status: **package copy executed and verified; release workflow and remaining
current documentation pending** (2026-11-07). The new package exists, is public
and is linked to `EQProxy/EQProxy`. This page is no longer plan-only. The
migration is **not complete for future releases**: the release workflow still
publishes to the old namespace.

## 0. Execution record (2026-11-07)

The package copy is executed; the package is linked and public. This section
records what ran; the remaining work is listed after the evidence.

- `ghcr.io/eqproxy/eqproxy` is public, reports
  `repository.full_name = EQProxy/EQProxy` and has 13 versions. Its owner is
  the **user** `EQProxy`.
- These tags were copied without rebuilding:

  | Tag | Index digest |
  |---|---|
  | `0.9.1` | `sha256:95170d9a19dc089ccf876a6ae91322e826b64086d7d07763a5462685e2ad775f` |
  | `0.9.2` | `sha256:6392aa3018c71a1bc87b3df22ec9e3d13a5b306e15fa66cbe4f819a49e77ea3f` |
  | `latest` | `sha256:6392aa3018c71a1bc87b3df22ec9e3d13a5b306e15fa66cbe4f819a49e77ea3f` |

- Anonymous pull and execution: after `docker logout ghcr.io`,
  `docker pull ghcr.io/eqproxy/eqproxy:0.9.2` succeeded and
  `docker run --rm ghcr.io/eqproxy/eqproxy:0.9.2 --version` returned
  `eqproxy 0.9.2`. The index lists `linux/amd64`, `linux/arm64` and two
  attestations.
- Reproducible check: `.github/scripts/check-ghcr-manifest.py` (commit
  `24e1a10` on `harden`; not in this review branch) fetches an anonymous
  token, hashes the exact index bytes, compares the expected digest, checks
  `Docker-Content-Digest`, and requires both platforms plus two attestations.
  Live passes exist for all three tags.
- Temporary migration workflow: run
  [42372923471](https://github.com/EQProxy/EQProxy/actions/runs/42372923471)
  on `migrate/ghcr-eqproxy`. The copy and digest-verification steps succeeded;
  the later **Make the package public** step returned HTTP 404. The package is
  already public, so that step was unnecessary; the workflow is red and must
  not be cited as a green run. The temporary branch and workflow file were
  deleted.
- The old package remains public with 130 versions and no repository link. It
  is still the archive for every tag not copied.

Remaining work, in order:

1. **Current release blocker:** `.github/workflows/release.yml` still logs in
   with `GHCR_TOKEN` and pushes `ghcr.io/exyonq/eqproxy`. Change it to
   `GITHUB_TOKEN` with `packages: write` on `ghcr.io/eqproxy/eqproxy`, and
   delete the `GHCR_TOKEN` guard. Not executed.
2. Apply the current documentation change and finish `CONTRIBUTING.md`,
   `CHANGELOG.md`, `repo.md`, `docs/docker-hub-readme.md` and
   `docker/Dockerfile`. The README change is local commit `89bc0a3`; it is not
   merged in this review branch. Not executed.
3. Decide whether to copy the remaining historical tags. Do not delete the old
   package until every tag that must remain available is copied; GHCR has no
   redirects. Not executed.
4. Cut a real `v*` release on the new namespace and verify GHCR, Docker Hub and
   Scout before calling the migration complete. Not executed.

## 1. The target is a user account, not an organization

`EQProxy` is a **user account**, not an organization. Verified on 2026-11-07:

```console
$ gh api users/EQProxy --jq '.login, .type'
EQProxy
User
$ gh api repos/EQProxy/EQProxy --jq '.owner.login, .owner.type'
EQProxy
User
```

An organization cannot reuse the login `EQProxy` while the user account
exists. This runbook therefore targets the user account. If an organization
is the real goal, create it under a different login and redo this plan
against that namespace; the mechanics below are the same, but the account
that owns the package and the repository changes.

## 2. Why this is a migration, not a package transfer

GitHub does **not** move a GHCR package between accounts. There is no
`Transfer ownership` action in the package settings UI and no REST or GraphQL
endpoint for it: the package settings Danger Zone only deletes, and the
Packages API only lists, gets, deletes and restores packages. A repository
transfer does not move a package, and GHCR does not redirect the old
namespace.

The migration therefore:

1. **copies** the existing OCI indexes to `ghcr.io/eqproxy/eqproxy`, keeping
   their digests;
2. links the new package to `EQProxy/EQProxy` and grants Actions access;
3. switches the workflow to `GITHUB_TOKEN` with `packages: write`;
4. retires the old package only after a deprecation window.

`crane copy` is the chosen tool because its contract is "retaining the digest
value" (checked with `crane copy --help` on 2026-11-07,
`crane` at `/opt/homebrew/bin/crane`). `docker buildx imagetools create
--dry-run` was also checked against the 0.9.2 index: it proposed the same
four manifests (amd64, arm64 and the two attestations) without pushing. That
dry run is a pre-check, not evidence that the copy happened.

## 3. Verified state

State on 2026-11-07 after the copy:

| Item | State |
|---|---|
| New package | `ghcr.io/eqproxy/eqproxy`, owner `EQProxy` (user), public, 13 versions; linked to `EQProxy/EQProxy` |
| Old package | `ghcr.io/exyonq/eqproxy`, owner `ExyonQ` (user), public, 130 versions; no repository link |
| 0.9.1 index | `sha256:95170d9a19dc089ccf876a6ae91322e826b64086d7d07763a5462685e2ad775f` |
| 0.9.2 index | `sha256:6392aa3018c71a1bc87b3df22ec9e3d13a5b306e15fa66cbe4f819a49e77ea3f` |
| 0.9.2 manifests | `linux/amd64` + `linux/arm64` + two attestation manifests |
| Repository | `EQProxy/EQProxy`, owner `EQProxy` (user) |
| Current workflow | **pending**: still pushes GHCR with `GHCR_TOKEN` (classic PAT of `ExyonQ`, `write:packages`); Docker Hub mirror `exyonq/eqproxy` with `DOCKERHUB_USERNAME`/`DOCKERHUB_TOKEN` |
| Current docs | README update is local commit `89bc0a3` on `harden`, not in this review branch; CONTRIBUTING, CHANGELOG, repo.md, Dockerfile and the Docker Hub page still use `ghcr.io/exyonq`. Historical release/evidence pages keep their publication-time refs |
| Browser session | only `Doken-Tokuyama`; it was not used for the copy |

The package owner (`ExyonQ`), the target account (`EQProxy`), the repository
owner (`EQProxy`) and the browser session (`Doken-Tokuyama`) are different
identities. The copy and link were executed by a temporary workflow with
`GITHUB_TOKEN`; the **EQProxy** session was not needed. Retiring the old
package still requires **ExyonQ**.

## 4. Target state

| Item | State on 2026-11-07 |
|---|---|
| Package | **reached**: `ghcr.io/eqproxy/eqproxy`, owner `EQProxy`, **public** |
| Link | **reached**: package connected to `EQProxy/EQProxy` |
| Actions access | **effective**: the temporary workflow copied with `GITHUB_TOKEN`; the UI role **Write** was not separately inspected |
| Workflow | **pending**: `release.yml` still uses `GHCR_TOKEN` and the old namespace |
| Docker Hub | unchanged: `exyonq/eqproxy`, same digests |
| Old package | retained as an archive during the deprecation window; not deleted |
| Digests | unchanged; `0.9.2`/`latest` stay `sha256:6392aa30…ea3f`, `0.9.1` stays `sha256:95170d9a…775f` |

The executed copy produced a public package, so no visibility change was
needed. Do not rely on that for a future copy: an explicit `PATCH` with
`GITHUB_TOKEN` returned 404, and a package can be private. Check `visibility`
and an anonymous pull after any copy; if it is private, use the EQProxy
package settings UI.

## 5. Decisions

1. **Copy by digest, do not rebuild.** Rebuilding would change the digests and
   invalidate the Scout reports and the release record. `crane copy` copies
   the index and its manifests without rebuilding them.
2. **Copy `0.9.1`, `0.9.2` and `latest` first.** They cover the current release
   records and pull target. Copying all 130 tags is optional while the old
   package is kept; if it will be deleted, copy every tag that must remain
   available first.
3. **Use `GITHUB_TOKEN`, not a PAT.** The executed copy did this from a
   temporary workflow. Once the package belongs to the same account as the
   repository and is linked, the image job can push with `packages: write`.
   This removes the long-lived `GHCR_TOKEN`; the release workflow still has to
   be changed.
4. **Keep Docker Hub as `exyonq/eqproxy`.** The Hub mirror is a separate
   registry and does not move with GHCR.
5. **Do not rewrite historical evidence.** `docs/release-0.9.1.md`,
   `docs/release-0.9.2.md` and `docs/evidence/**` record the refs used at
   publication time. Add a dated migration note after the copy; do not edit
   the old commands and digests.

## 6. Access and preconditions

The copy is already executed; this list records what was needed and what is
still needed for the remaining steps.

- The executed copy used a temporary workflow with `GITHUB_TOKEN` and
  `packages: write`. It did **not** use `GHCR_MIGRATION_TOKEN`, `crane auth
  login` or the EQProxy browser session.
- The package is now linked and public; effective write access is proven by
  the successful copy. The UI role **Write** was not separately inspected.
- `crane` and `docker buildx` remain useful for independent checks; the
  reproducible check is `.github/scripts/check-ghcr-manifest.py`.
- An EQProxy session is still required for manual package settings if
  visibility or access ever needs changing, and an ExyonQ session is required
  to retire the old package. Neither was needed for the copy.
- The release workflow/docs change is **not merged**; this is the current
  blocker for the next release.
- `v*` tags are not frozen and no cutover was announced for future releases.
- Docker Hub secrets are unchanged.
- A maintenance window is still required for a full historical-tag copy.

## 7. Cutover

> Executed on 2026-11-07: 7.1–7.4, plus the package link and public state
> described in 7.5–7.6. The 7.5 UI steps were not run; 7.6's PATCH returned
> 404. 7.7–7.9 remain pending; 7.7 is the current release blocker.

### 7.1 Record the source digest (executed)

```console
$ docker buildx imagetools inspect --raw ghcr.io/exyonq/eqproxy:0.9.2 | sha256sum
6392aa3018c71a1bc87b3df22ec9e3d13a5b306e15fa66cbe4f819a49e77ea3f  -
$ crane digest ghcr.io/exyonq/eqproxy:0.9.2
sha256:6392aa3018c71a1bc87b3df22ec9e3d13a5b306e15fa66cbe4f819a49e77ea3f
```

Stop if either value differs from the release record.

### 7.2 Authentication actually used (executed)

The copy ran in the temporary workflow `migrate/ghcr-eqproxy` in
`EQProxy/EQProxy` and logged in with the repository `GITHUB_TOKEN` carrying
`packages: write`. No `GHCR_MIGRATION_TOKEN`, PAT, `crane auth login` or
EQProxy browser session was used.

Run 42372923471 copied and verified the manifests. Its visibility step then
failed with HTTP 404; the package was already public, so no retry is needed.

### 7.3 Copy the current release tags (executed)

```console
$ for tag in 0.9.1 0.9.2 latest; do
    crane copy "ghcr.io/exyonq/eqproxy:$tag" "ghcr.io/eqproxy/eqproxy:$tag"
  done
```

Optional full archive (all 130 tags, including `-candidate` tags); not executed:

```console
$ crane copy --all-tags ghcr.io/exyonq/eqproxy ghcr.io/eqproxy/eqproxy
```

The executed workflow copied `0.9.1`, `0.9.2` and `latest` without a rebuild;
the package is linked to `EQProxy/EQProxy` and public. For a future empty
target, do not assume the first push links or publishes the package: verify
both.

### 7.4 Verify the copy before touching the UI (executed)

```console
$ docker buildx imagetools inspect --raw ghcr.io/eqproxy/eqproxy:0.9.2 | sha256sum
6392aa3018c71a1bc87b3df22ec9e3d13a5b306e15fa66cbe4f819a49e77ea3f  -
$ docker buildx imagetools inspect ghcr.io/eqproxy/eqproxy:0.9.2
# must list linux/amd64, linux/arm64 and the two attestation manifests
$ docker pull ghcr.io/eqproxy/eqproxy:0.9.2
$ docker run --rm ghcr.io/eqproxy/eqproxy:0.9.2 --version
eqproxy 0.9.2
```

The reproducible check now repeats the index, platform and attestation checks
without Docker. The checker is in the `harden` branch (`24e1a10`); this review
branch does not include it yet, so run the command from `harden` until the
branches are merged:

```console
$ python3 .github/scripts/check-ghcr-manifest.py \
    --tag 0.9.2 --tag latest \
    --expected-digest sha256:6392aa3018c71a1bc87b3df22ec9e3d13a5b306e15fa66cbe4f819a49e77ea3f
```

If the index digest differs, **stop**: do not link, do not merge the
workflow, do not delete the old package. Investigate the copy first.

### 7.5 Link and Actions access (reached)

The package reports `repository.full_name = EQProxy/EQProxy`. The temporary
workflow copied with `GITHUB_TOKEN`, so effective write access is proven. The
package-settings role **Write** was not separately inspected; the UI steps
below remain the way to confirm it.

As **EQProxy**:

1. Open `https://github.com/users/EQProxy/packages/container/eqproxy/settings`.
2. **Connect repository** → `EQProxy/EQProxy`. Verify that
   `repository.full_name` becomes `EQProxy/EQProxy`.
3. **Manage Actions access** → **Add repository** → `EQProxy/EQProxy` →
   role **Write**. If the package already inherits access from the linked
   repository, confirm the effective role is Write; the explicit entry is the
   requested state.

### 7.6 Visibility (public; API caveat)

`gh api /users/EQProxy/packages/container/eqproxy` reports
`visibility: public`, and an anonymous pull works. The temporary workflow's
`PATCH` with `GITHUB_TOKEN` returned 404; the package was already public, so
that failure did not block the copy. Do not use that PATCH as a post-copy
check. If a future package is private, change it in the EQProxy UI after the
digest and secret checks.

### 7.7 Apply and merge the workflow and documentation change (pending; current blocker)

This is the current blocker for future releases. `release.yml` in this branch
still logs in with `GHCR_TOKEN` and pushes `ghcr.io/exyonq/eqproxy`; the
README change is local commit `89bc0a3` on `harden` and is not in this branch.
The rest of the current docs also still use the old namespace. The image job
then:

- logs in to `ghcr.io` as `eqproxy` with `GITHUB_TOKEN`;
- pushes `ghcr.io/eqproxy/eqproxy` with `packages: write`;
- keeps Docker Hub as `exyonq/eqproxy`;
- has no `GHCR_TOKEN` guard and no PAT.

Do not tag a release between the start of the copy and this merge; the old
workflow cannot publish to the transferred name, and the new one must not
run before the new package exists.

### 7.8 First release is the end-to-end gate (pending)

Cut the next version (a real `v*` tag, not a reused 0.9.2 tag) and verify the
whole pipeline on the new namespace. The migration is not "done" until this
release passes:

```console
$ docker buildx imagetools inspect --raw ghcr.io/eqproxy/eqproxy:<version> | sha256sum
# must equal the candidate index the Scout job scanned
$ docker pull ghcr.io/eqproxy/eqproxy:<version>
$ docker run --rm ghcr.io/eqproxy/eqproxy:<version> --version
$ docker pull exyonq/eqproxy:<version>
# Docker Hub must still serve the same digest
```

The release workflow's own promotion check compares the scanned candidate
digest with every published tag; a mismatch fails the tag.

### 7.9 Retire the old package (pending)

- The old package is still public with 130 versions and no linked repository;
  it still serves every tag, including the 127 versions not copied.
- Keep `ghcr.io/exyonq/eqproxy` frozen for a deprecation window. Its `latest`
  stays at 0.9.2 and does not receive new releases.
- Add a dated note to `docs/release-0.9.1.md` and `docs/release-0.9.2.md`
  saying that the package was copied to `ghcr.io/eqproxy/eqproxy` and that
  the publication-time references in those records are historical.
- After the window, delete the old package as **ExyonQ** only if every tag
  that must remain available has been copied. GHCR has no redirects: every old
  reference stops resolving at that moment, including any tag not copied.
  Announce the cutover before deleting.
- Delete the `GHCR_TOKEN` repository secret and revoke the PAT. Delete
  `GHCR_MIGRATION_TOKEN` after the first successful release.

## 8. Verification checklist

State on 2026-11-07:

| Check | State | Evidence |
|---|---|---|
| New 0.9.2 index digest equals the recorded digest | **Done** | checker and release record: `sha256:6392aa30…ea3f` |
| 0.9.1 index digest | **Done** | `sha256:95170d9a…775f` |
| Both platforms and both attestations copied | **Done** | 2 attestations; `b984b522…` amd64, `d8b681d5…` arm64 |
| New image runs | **Done** | anonymous `docker run ghcr.io/eqproxy/eqproxy:0.9.2 --version` → `eqproxy 0.9.2` |
| Package linked to the repository | **Done** | API `repository.full_name = EQProxy/EQProxy` |
| Actions effective write | **Done** | temporary copy with `GITHUB_TOKEN`; UI role not inspected |
| Visibility is Public | **Done** | API `visibility: public`; anonymous pull passes |
| Workflow uses `GITHUB_TOKEN` | **Pending** | temporary copy did; `release.yml` still uses `GHCR_TOKEN` |
| Scout digest == published tag == Hub mirror | **Pending** | first release on the new namespace not cut |
| `GHCR_TOKEN` gone | **Pending** | still used by `release.yml`; secret rotation pending |
| Current docs point at the new namespace | **Partial** | README `89bc0a3`; CONTRIBUTING, CHANGELOG, repo.md, Dockerfile and the Hub page pending |
| Old refs behave as documented | **Pending decision** | old package still serves 130 versions; no redirects |
| Migration run is green | **No** | run `42372923471` failed at the visibility PATCH; copy/digest steps passed |

## 9. Rollback and abort

Current position: the copy is verified, the old package still serves every
tag, and the workflow/docs are not merged. The "after the copy, before the
merge" rollback path applies.

- **Before the copy:** abort; nothing changed.
- **After the copy, before the merge:** the old package still serves every
  existing tag. Fix the copy or retry; the docs/workflow are not merged.
- **After the merge, first release fails:** the old package and Docker Hub
  still serve 0.9.2. Fix the link/Actions access/`GITHUB_TOKEN` permission,
  or revert the workflow and docs to the old namespace while the old package
  exists. Do not delete the old package during this window.
- **After the new release is verified:** keep the old package for the
  deprecation window; a rollback means pointing the docs/workflow back at the
  old refs, which still exist until deletion.

Deleting the old package is the only irreversible step; it happens last.

## 10. Risks and limitations

- **No redirects.** GHCR does not forward `ghcr.io/exyonq/eqproxy` to the new
  namespace. Users must update their references; deleting the old package
  breaks them.
- **The target is a user, not an organization.** The target was described as
  an organization, but GitHub reports the account as a user. An organization
  with that login cannot be created while the user exists.
- **Multiple identities.** The package owner (`ExyonQ`), target account
  (`EQProxy`) and browser session (`Doken-Tokuyama`) are different. The copy
  did not need EQProxy; the remaining UI checks and the retirement do.
- **The visibility API is not a reliable post-copy check.** The explicit
  `PATCH` with `GITHUB_TOKEN` returned 404. The package was already public;
  use the API `visibility` field and an anonymous pull, not that PATCH.
- **The migration run is red.** Run `42372923471` copied and verified the
  manifests, then failed at the visibility step. Do not cite it as a green
  end-to-end run; the current package state is verified separately.
- **The release workflow is still old.** This is the current blocker: future
  tags still publish to `ghcr.io/exyonq` until `release.yml` is changed and
  merged. The temporary copy is not the release pipeline.
- **Only three tags were copied.** The old package is the archive for the other
  127 versions. Copy every tag that must remain before deletion.
- **The full archive copy is slow.** Copying all 130 versions needs more time
  than the three current tags and should be a deliberate maintenance step.
- **Docker Hub page timing.** `docs/docker-hub-readme.md` renders on the next
  stable release; until then the Hub page still describes the old GHCR name.
- **Historical evidence stays historical.** The old refs in release records
  and evidence are intentionally not rewritten; a dated note explains them.
- **The migration is not complete.** The package is migrated; the release
  workflow, remaining current docs, historical-tag decision and retirement are
  not.

## 11. Checklist

- [x] Temporary workflow with `GITHUB_TOKEN` and `packages: write` executed
- [ ] `v*` tags frozen; cutover announced
- [x] `0.9.1`, `0.9.2` and `latest` copied
- [ ] If the old package will be deleted, every tag to keep was copied
- [x] New index digests and four manifests verified
- [x] New package linked to `EQProxy/EQProxy`
- [x] Effective Actions write proven; UI role Write not separately inspected
- [x] New package visibility = Public; anonymous pull passed
- [ ] Release workflow uses `GITHUB_TOKEN` and the new namespace
- [ ] Current docs updated; no `GHCR_TOKEN`
- [ ] First release on the new namespace verified (GHCR + Hub + Scout)
- [ ] `GHCR_TOKEN` secret deleted and PAT revoked
- [ ] Historical-tag decision recorded; old package frozen; dated notes added
- [ ] Old package deleted only after the deprecation window and the tag check
