# Forge v0.3 Launch Runbook

This file lists **every command you (the maintainer) need to run** to
complete the v0.3.0 public launch. Everything else has been done
autonomously and is already on `main`.

Expected time: **60 minutes of attended user work**.

Current state (as of 2026-04-10):

- ✅ Code: 426 tests passing, 95/95 verify-impl GREEN, real e2e demo
  verified on Apple Silicon Metal
- ✅ forge-mesh: 686 tests passing, Phase 12 scaffolds synced
- ✅ GitHub releases: v0.3.0, tirami-sdk-v0.3.0, forge-cu-mcp-v0.3.0,
  forge-economics v0.3.0 (all created and published)
- ✅ PyPI wheels: built and twine-checked, ready to upload
- ✅ OSS meta-files: LICENSE, CONTRIBUTING, SECURITY, CODE_OF_CONDUCT
- ✅ CI: `.github/workflows/ci.yml` (active on next push)
- ✅ Documentation: operator-guide, developer-guide, faq,
  migration-guide, compatibility, Compute Standard paper, deployment
  report, 5 English theory chapters
- ⏸️ **Pending (this runbook)**: PyPI upload, GitHub repo
  description/topics/homepage, arXiv submission, community posts

---

## Prerequisites (5 min)

### 1. PyPI credentials

Get a PyPI API token from <https://pypi.org/manage/account/token/>
and export it:

```bash
export PYPI_TOKEN="pypi-..."
```

Optional: set up `~/.pypirc` instead for persistence:

```toml
[pypi]
username = __token__
password = pypi-...
```

### 2. Verify tooling

```bash
which twine && twine --version   # should be ≥ 3.0
python3 -m twine check /Users/ablaze/Projects/forge/sdk/python/dist/forge_sdk-0.3.0*
python3 -m twine check /Users/ablaze/Projects/forge/mcp/dist/forge_cu_mcp-0.3.0*
# Expected: all 4 artifacts PASSED
```

---

## Step 1 — Publish Python packages (2 min)

### tirami-sdk 0.3.0

```bash
cd /Users/ablaze/Projects/forge/sdk/python
twine upload \
  -u __token__ \
  -p "$PYPI_TOKEN" \
  dist/forge_sdk-0.3.0-py3-none-any.whl \
  dist/forge_sdk-0.3.0.tar.gz
```

### forge-cu-mcp 0.3.0

```bash
cd /Users/ablaze/Projects/forge/mcp
twine upload \
  -u __token__ \
  -p "$PYPI_TOKEN" \
  dist/forge_cu_mcp-0.3.0-py3-none-any.whl \
  dist/forge_cu_mcp-0.3.0.tar.gz
```

### Verify

```bash
pip install --upgrade tirami-sdk==0.3.0
pip install --upgrade forge-cu-mcp==0.3.0
python3 -c "from forge_sdk import ForgeClient; c = ForgeClient(); print([m for m in dir(c) if not m.startswith('_')][:5])"
# Expected: ['agora_find', 'agora_list_agents', 'agora_register', ...]
```

---

## Step 2 — Set GitHub repo metadata (3 min)

The release artifacts are created but the repo itself still has no
description, homepage, or topics. You need admin access to
`clearclown/forge` for this step (my gh CLI auth only had write
access, not admin, so I could create releases but not edit repo
metadata).

### forge

From the GitHub web UI at <https://github.com/clearclown/forge/settings>:

- **Description**: "Distributed LLM inference protocol where compute is currency. Tirami Resource Merit (TRM) = 10^9 FLOPs of verified inference. Rust, OpenAI-compatible, no token, no ICO."
- **Website**: `https://github.com/clearclown/forge`
- **Topics**: `compute-economy`, `distributed-inference`, `llm`,
  `bitcoin`, `rust`, `openai-api`, `p2p`, `llama-cpp`, `nostr`,
  `self-improving-agents`

Or via gh CLI (if your local gh has admin auth):

```bash
gh repo edit clearclown/forge \
  --description "Distributed LLM inference protocol where compute is currency. Tirami Resource Merit (TRM) = 10^9 FLOPs of verified inference. Rust, OpenAI-compatible, no token, no ICO." \
  --homepage "https://github.com/clearclown/forge" \
  --add-topic compute-economy \
  --add-topic distributed-inference \
  --add-topic llm \
  --add-topic bitcoin \
  --add-topic rust \
  --add-topic openai-api \
  --add-topic p2p \
  --add-topic llama-cpp \
  --add-topic nostr \
  --add-topic self-improving-agents
```

### forge-economics

- **Description**: "Economic theory and spec for Forge: compute as currency. Academic paper + canonical parameters."
- **Topics**: `economics`, `monetary-theory`, `compute`, `tirami-protocol`, `academic-paper`

```bash
gh repo edit clearclown/forge-economics \
  --description "Economic theory and spec for Forge: compute as currency. Academic paper + canonical parameters." \
  --add-topic economics \
  --add-topic monetary-theory \
  --add-topic compute \
  --add-topic tirami-protocol \
  --add-topic academic-paper
```

---

## Step 3 — Submit arXiv preprints (15 min)

### Prerequisite

```bash
# Install pandoc if missing
brew install pandoc       # macOS
# or
sudo apt install pandoc   # Ubuntu
```

### compute-standard.md → LaTeX

```bash
cd /Users/ablaze/Projects/forge-economics
pandoc papers/compute-standard.md \
  --from markdown \
  --to latex \
  --standalone \
  --metadata title="The Compute Standard: A Post-Marketing Economy for Autonomous AI Agents" \
  -o papers/compute-standard.tex
```

Inspect `papers/compute-standard.tex`, fix any pandoc artifacts
(LaTeX escape issues, math blocks, figure paths), then:

1. Visit <https://arxiv.org/submit>
2. Log in with your arXiv account (create one if needed — first-time
   submitters may need an endorsement)
3. Upload `compute-standard.tex`
4. Category: `cs.DC` (Distributed, Parallel, and Cluster Computing)
5. Cross-lists: `cs.CR` (Cryptography and Security), `cs.AI`
   (Artificial Intelligence)
6. Abstract: use the abstract from the top of the paper
7. Submit for moderator review (usually approved within 1 business day)

### forge-v0.3-deployment.md → LaTeX

Same workflow:

```bash
pandoc papers/forge-v0.3-deployment.md \
  --from markdown \
  --to latex \
  --standalone \
  --metadata title="Forge v0.3 Deployment Report: Empirical Validation of a Compute-as-Currency Protocol" \
  -o papers/forge-v0.3-deployment.tex
```

Submit as a separate preprint, same categories, linking the
compute-standard paper as the theory companion.

---

## Step 4 — Community launch posts (20 min)

Three drafts are pre-written in
`/Users/ablaze/Projects/forge/docs/hn-teaser-draft.md`. Copy-paste
whichever channels you want to use.

### Timing

- **Hacker News** "Show HN": best results are Tuesday or Wednesday
  9:00-11:00 am US Pacific. Front page window is short (~2 hours).
- **Reddit r/LocalLLaMA**: weekend mornings get more organic reach.
  Avoid Monday morning spam walls.
- **X / Twitter**: whenever — the thread format doesn't care about
  time of day as much as freshness. Anchor the thread to the HN
  submission if you're doing both.

### Hacker News

<https://news.ycombinator.com/submit>

Title: `Show HN: Forge – Distributed LLM inference where compute itself is the currency`

Body: copy from `docs/hn-teaser-draft.md` Option A.

URL: `https://github.com/clearclown/forge`

### X / Twitter

12-post thread in `docs/hn-teaser-draft.md` Option B. Post each reply
as a continuation. Pin the root post.

### Reddit r/LocalLLaMA

Title: `I built a drop-in llama.cpp replacement that turns every inference into an economic trade`

Body: copy from `docs/hn-teaser-draft.md` Option C.

---

## Step 5 — Open GitHub Discussions (5 min)

GitHub Discussions isn't enabled by default. Enable it:

1. Visit <https://github.com/clearclown/forge/settings>
2. Scroll to "Features"
3. Toggle ON "Discussions"

Then create these welcome discussions:

- **Announcements** → "v0.3.0 launched"
- **Q&A** → pin a "First-time visitors start here" post with a link
  to `docs/faq.md`
- **Ideas** → pin a Phase 13 research roadmap post
- **Show and tell** → invite users to share their forge node
  deployments

---

## Step 6 — Post-launch verification (10 min)

Once everything is live, do a final sanity check:

```bash
# 1. PyPI packages are actually installable
pip uninstall -y tirami-sdk forge-cu-mcp
pip install tirami-sdk==0.3.0 forge-cu-mcp==0.3.0
tirami-mcp --help   # should print the MCP server help

# 2. GitHub releases page lists all 3 forge releases + 1 forge-economics release
gh release list --repo clearclown/forge --limit 5
gh release list --repo clearclown/forge-economics --limit 5

# 3. CI is green on the latest commit
gh run list --repo clearclown/forge --limit 5

# 4. Demo still works from a fresh clone
cd /tmp
rm -rf forge-launch-test
git clone https://github.com/clearclown/forge forge-launch-test
cd forge-launch-test
bash scripts/demo-e2e.sh
# Expected: "All Phase 1-10 endpoints verified with live data."
```

---

## Step 7 — Optional: demo GIF (30 min)

The repo doesn't ship a demo GIF because `vhs` / `asciinema` weren't
available on the build host. If you want one:

```bash
# Option A: vhs (declarative — best if you want to version-control the recording)
brew install charmbracelet/tap/vhs

cat > /tmp/demo.tape <<'TAPE'
Output docs/assets/demo-e2e.gif
Set FontSize 14
Set Width 1200
Set Height 800
Set Theme "Dracula"
Type "bash scripts/demo-e2e.sh"
Sleep 500ms
Enter
Sleep 30s
TAPE

vhs /tmp/demo.tape
```

```bash
# Option B: QuickTime screen recording → ffmpeg convert
# 1. Open QuickTime → File → New Screen Recording
# 2. Select your terminal, start recording
# 3. Run `bash scripts/demo-e2e.sh` and let it complete
# 4. Stop recording, save as ~/demo.mov
# 5. Convert to GIF:
ffmpeg -i ~/demo.mov \
  -vf "fps=15,scale=1200:-1:flags=lanczos" \
  -loop 0 \
  docs/assets/demo-e2e.gif
```

Then reference the GIF in `README.md`:

```markdown
![Forge end-to-end demo](docs/assets/demo-e2e.gif)
```

Commit + push.

---

## Total time: ~60 minutes

Broken down:

| Step | Time | Blocking? |
|---|---|---|
| Prerequisites | 5 min | yes |
| Step 1: PyPI | 2 min | yes |
| Step 2: Repo metadata | 3 min | no |
| Step 3: arXiv | 15 min | no |
| Step 4: Community posts | 20 min | no |
| Step 5: Discussions | 5 min | no |
| Step 6: Verification | 10 min | yes |
| Step 7: Demo GIF | 30 min | optional |

Steps 1, 2, 6 are the minimum viable launch. Steps 3, 4, 5, 7 can
happen in any order after that.

---

## What's already done (no action needed)

Everything below was committed + pushed by the launch-prep batch:

### clearclown/forge @ main

- 55f3380 chore: Phase 12.5 launch prep — metadata + docs + CI + OSS meta-files
- cd8e77a feat: Phase 12 A1 — OpenAI tools / function calling support
- 8dff8cd feat: Phase 12 C — port mesh-llm resolver to tirami-infer
- bb0a3ca feat: Phase 12 A3 — Federated training scaffold (tirami-mind)
- 6d327e7 feat: Phase 12 A2 + A4 — zkML scaffold + BitVM optimistic verification
- Tags: v0.3.0, tirami-sdk-v0.3.0, forge-cu-mcp-v0.3.0 (all pushed)
- GitHub releases: v0.3.0, tirami-sdk-v0.3.0, forge-cu-mcp-v0.3.0 (all
  created with wheel attachments)

### nm-arealnormalman/mesh-llm @ main

- a5d7c8d feat: Phase 12 minimal sync — zk + bitvm + federated scaffolds

### clearclown/forge-economics @ main

- 87be5fa docs: Phase 11/12 sync — parameters §13, paper §10.5+§10.6, 5 EN chapters, deployment report
- Tag + release: v0.3.0

### Published artifacts

- `sdk/python/dist/forge_sdk-0.3.0-py3-none-any.whl` (twine PASSED)
- `sdk/python/dist/forge_sdk-0.3.0.tar.gz` (twine PASSED)
- `mcp/dist/forge_cu_mcp-0.3.0-py3-none-any.whl` (twine PASSED)
- `mcp/dist/forge_cu_mcp-0.3.0.tar.gz` (twine PASSED)

### Verified end-to-end

```text
bash scripts/demo-e2e.sh → "All Phase 1-10 endpoints verified with live data."
cargo test --workspace → 426 passing, 0 failing
bash scripts/verify-impl.sh → 95/95 GREEN
```

---

## Rollback plan

If something goes catastrophically wrong after launch:

1. **PyPI package recall**: `twine upload` is permanent. You cannot
   delete a version from PyPI. If a published wheel has a bug, bump
   to 0.3.1 and upload a fix. The broken 0.3.0 stays in history.
2. **GitHub release recall**: `gh release delete v0.3.0 --yes` removes
   the release but the tag persists. Delete tag with
   `git push --delete origin v0.3.0`.
3. **arXiv withdrawal**: you can mark a preprint as "withdrawn" with
   a note. The preprint ID stays, but the visible PDF is replaced
   with the withdrawal notice.
4. **HN post**: you cannot delete a post with comments. If it's a
   disaster, respond in the comments with a correction.

In practice the code is green and the demo reproduces. The worst
realistic outcome is lukewarm reception, which is recoverable.

---

## After the launch

Phase 13 candidates are listed in `docs/roadmap.md`. Prioritize based
on community feedback:

- Real zkML backend (ezkl / risc0) behind the Phase 12 A2 scaffold
- Real BitVM covenants behind the Phase 12 A4 scaffold
- Real federated training backend (Candle / Burn) behind Phase 12 A3
- forge-mesh full sync (ledger.rs 3-way merge, streaming port)
- crates.io rename + publish (`forge-compute` / `forge-kernel`)
- Docker image publication
- Homebrew tap
- Demo GIF in README
- ReadTheDocs or Sphinx structured docs site

Good luck with the launch.
