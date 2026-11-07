# Migrating the GHCR package to the EQProxy account

Language: **English** (public documentation).

Status: **planned, not executed** (2026-11-07). This page is the runbook. It
does not claim that the migration has happened. The current published docs and
release workflow remain on `ghcr.io/exyonq`; the new-namespace change is
deferred until the copy and link are verified, and must not be merged before the
new package exists.

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

## 3. Verified starting point

| Item | State on 2026-11-07 |
|---|---|
| Old package | `ghcr.io/exyonq/eqproxy`, owner `ExyonQ` (user), public, 130 versions |
| Old package link | `repository.full_name = null` (not linked to a repository) |
| 0.9.2 index | `sha256:6392aa3018c71a1bc87b3df22ec9e3d13a5b306e15fa66cbe4f819a49e77ea3f` |
| 0.9.2 manifests | `linux/amd64` + `linux/arm64` + two attestation manifests |
| Repository | `EQProxy/EQProxy`, owner `EQProxy` (user) |
| Repository access | the `ExyonQ` token has `push`, not `admin` (`permissions.admin = false`) |
| Current workflow | pushes GHCR with `GHCR_TOKEN` (classic PAT of `ExyonQ`, `write:packages`); Docker Hub mirror `exyonq/eqproxy` with `DOCKERHUB_USERNAME`/`DOCKERHUB_TOKEN` |
| Browser session | only `Doken-Tokuyama`; it is not a package admin and its repo settings request returns 404 |
| Current docs | README, CONTRIBUTING, CHANGELOG, repo.md, Dockerfile and the Docker Hub page keep `ghcr.io/exyonq`; the new-namespace change is deferred until the copy is verified. Historical release/evidence pages keep their publication-time refs |

The package owner (`ExyonQ`), the target account (`EQProxy`), the repository
owner (`EQProxy`) and the browser session (`Doken-Tokuyama`) are four
different identities. The UI steps require the **EQProxy** session; the old
package's retirement requires the **ExyonQ** session.

## 4. Target state

| Item | Target |
|---|---|
| Package | `ghcr.io/eqproxy/eqproxy`, owner `EQProxy`, **public** |
| Link | package connected to `EQProxy/EQProxy` |
| Actions access | repository `EQProxy/EQProxy` with role **Write** |
| Workflow | `GITHUB_TOKEN` + `packages: write`, no `GHCR_TOKEN` |
| Docker Hub | unchanged: `exyonq/eqproxy`, same digests |
| Old package | frozen as an archive during the deprecation window, then deleted (no redirect) |
| Digests | unchanged; the 0.9.2 index stays `sha256:6392aa30…ea3f` |

The new package is **private by default** when the first copy creates it.
Setting it public is an explicit step and is irreversible. Do it only after
the digest check passes and after confirming the image has no secrets.

## 5. Decisions

1. **Copy by digest, do not rebuild.** Rebuilding would change the digests and
   invalidate the Scout reports and the release record. `crane copy` copies
   the index and its manifests without rebuilding them.
2. **Copy `0.9.2` and `latest` first.** They are the current pull targets.
   Copying all 130 tags is optional while the old package is kept; if it will
   be deleted, copy every tag that must remain available first.
3. **Use `GITHUB_TOKEN`, not a PAT.** Once the package belongs to the same
   account as the repository and is linked, the image job can push with
   `packages: write`. This removes the long-lived `GHCR_TOKEN`.
4. **Keep Docker Hub as `exyonq/eqproxy`.** The Hub mirror is a separate
   registry and does not move with GHCR.
5. **Do not rewrite historical evidence.** `docs/release-0.9.1.md`,
   `docs/release-0.9.2.md` and `docs/evidence/**` record the refs used at
   publication time. Add a dated migration note after the copy; do not edit
   the old commands and digests.

## 6. Access and preconditions

- Freeze `v*` tags for the cutover window.
- An **EQProxy** classic PAT with `write:packages` (`GHCR_MIGRATION_TOKEN`),
  kept in the environment or a secret store, never in the repository.
  The old package is public, so reading it needs no scope; if that ever
  changes, add `read:packages`.
- `crane` installed and `docker buildx` available.
- A browser session for **EQProxy** to link the package, grant Actions
  access and set visibility. The current BrowserOS session is
  `Doken-Tokuyama`; it cannot do these steps.
- The workflow/documentation change prepared but **not merged**.
- Docker Hub secrets unchanged.
- A maintenance window; copying the two release tags is quick, copying all
  130 versions is longer.

## 7. Cutover

### 7.1 Record the source digest

```console
$ docker buildx imagetools inspect --raw ghcr.io/exyonq/eqproxy:0.9.2 | sha256sum
6392aa3018c71a1bc87b3df22ec9e3d13a5b306e15fa66cbe4f819a49e77ea3f  -
$ crane digest ghcr.io/exyonq/eqproxy:0.9.2
sha256:6392aa3018c71a1bc87b3df22ec9e3d13a5b306e15fa66cbe4f819a49e77ea3f
```

Stop if either value differs from the release record.

### 7.2 Authenticate as EQProxy

```console
$ echo "$GHCR_MIGRATION_TOKEN" | crane auth login ghcr.io -u eqproxy --password-stdin
```

The token is not printed and is not written to the repository.

### 7.3 Copy the current release tags

```console
$ for tag in 0.9.2 latest; do
    crane copy "ghcr.io/exyonq/eqproxy:$tag" "ghcr.io/eqproxy/eqproxy:$tag"
  done
```

Optional full archive (all 130 tags, including `-candidate` tags):

```console
$ crane copy --all-tags ghcr.io/exyonq/eqproxy ghcr.io/eqproxy/eqproxy
```

`crane copy` creates the new package on the first push. It does not link it
to a repository and does not make it public.

### 7.4 Verify the copy before touching the UI

```console
$ docker buildx imagetools inspect --raw ghcr.io/eqproxy/eqproxy:0.9.2 | sha256sum
6392aa3018c71a1bc87b3df22ec9e3d13a5b306e15fa66cbe4f819a49e77ea3f  -
$ docker buildx imagetools inspect ghcr.io/eqproxy/eqproxy:0.9.2
# must list linux/amd64, linux/arm64 and the two attestation manifests
$ docker pull ghcr.io/eqproxy/eqproxy:0.9.2
$ docker run --rm ghcr.io/eqproxy/eqproxy:0.9.2 --version
eqproxy 0.9.2
```

If the index digest differs, **stop**: do not link, do not merge the
workflow, do not delete the old package. Investigate the copy first.

### 7.5 Link the package and grant Actions access

As **EQProxy**:

1. Open `https://github.com/users/EQProxy/packages/container/eqproxy/settings`.
2. **Connect repository** → `EQProxy/EQProxy`. Verify that
   `repository.full_name` becomes `EQProxy/EQProxy`.
3. **Manage Actions access** → **Add repository** → `EQProxy/EQProxy` →
   role **Write**. If the package already inherits access from the linked
   repository, confirm the effective role is Write; the explicit entry is the
   requested state.

### 7.6 Make the new package public

As **EQProxy**, package settings → **Change visibility** → **Public**, after
re-checking the digest and that the image carries no secrets. This is
irreversible; the old package is public, and users must be able to pull the
new one without credentials.

### 7.7 Apply and merge the workflow and documentation change

Apply and merge only after 7.4–7.6 pass; the current tree still uses the old
namespace. The image job then:

- logs in to `ghcr.io` as `eqproxy` with `GITHUB_TOKEN`;
- pushes `ghcr.io/eqproxy/eqproxy` with `packages: write`;
- keeps Docker Hub as `exyonq/eqproxy`;
- has no `GHCR_TOKEN` guard and no PAT.

Do not tag a release between the start of the copy and this merge; the old
workflow cannot publish to the transferred name, and the new one must not
run before the new package exists.

### 7.8 First release is the end-to-end gate

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

### 7.9 Retire the old package

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

| Check | Evidence |
|---|---|
| New index digest equals the recorded 0.9.2 digest | `buildx imagetools inspect --raw` + `sha256sum` |
| Both platforms and both attestations copied | `buildx imagetools inspect` lists four manifests |
| New image runs | `docker run --rm ghcr.io/eqproxy/eqproxy:0.9.2 --version` → `eqproxy 0.9.2` |
| Package linked to the repository | API `repository.full_name = EQProxy/EQProxy`; package page shows the repo |
| Actions access is Write | package settings UI |
| Visibility is Public | package settings UI; anonymous `docker pull` works |
| Workflow uses `GITHUB_TOKEN` | run log: `Log in to GHCR (EQProxy package namespace)`, no `GHCR_TOKEN` |
| Scout digest == published tag == Hub mirror | release run's promotion/verification steps |
| `GHCR_TOKEN` gone | `gh secret list` has no `GHCR_TOKEN` |
| After applying the change, current docs point at the new namespace | `grep -rn 'ghcr.io/exyonq/eqproxy' README.md CONTRIBUTING.md CHANGELOG.md repo.md docs/docker-hub-readme.md docker/Dockerfile` is empty; historical pages keep their refs |
| Old refs behave as documented | before deletion they still serve the old package; after deletion they fail (no redirect) |

## 9. Rollback and abort

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
  (`EQProxy`) and browser session (`Doken-Tokuyama`) are different. The UI
  steps need the EQProxy session, which is not currently in the browser.
- **`GITHUB_TOKEN` needs the link and Actions access.** Pushing before the
  package is linked/authorised fails; the workflow fails loudly rather than
  falling back to a PAT.
- **Visibility is private by default.** The copy creates a private package;
  it must be made public explicitly.
- **Copying all versions is slow.** The old package is the archive for any
  tag not copied; copying every tag is optional but should be decided before
  deleting the old package.
- **Docker Hub page timing.** `docs/docker-hub-readme.md` renders on the next
  stable release; until then the Hub page still describes the old GHCR name.
- **Historical evidence stays historical.** The old refs in release records
  and evidence are intentionally not rewritten; the dated note explains them.
- **This page is a plan.** No copy, link, visibility change, merge, deletion
  or secret rotation has been executed.

## 11. Checklist

- [ ] `EQProxy` session and `GHCR_MIGRATION_TOKEN` available
- [ ] `v*` tags frozen; cutover announced
- [ ] Source index digest recorded and matches `sha256:6392aa30…ea3f`
- [ ] `0.9.2` and `latest` copied (and any additional tags chosen)
- [ ] If the old package will be deleted, every tag to keep was copied
- [ ] New index digest and four manifests verified
- [ ] New package linked to `EQProxy/EQProxy`
- [ ] Actions access `EQProxy/EQProxy` = Write
- [ ] New package visibility = Public
- [ ] Workflow/docs change merged; no `GHCR_TOKEN`
- [ ] First release on the new namespace verified (GHCR + Hub + Scout)
- [ ] `GHCR_TOKEN` secret deleted and PAT revoked
- [ ] Old package frozen; dated notes added to the release records
- [ ] Old package deleted only after the deprecation window and the tag check
