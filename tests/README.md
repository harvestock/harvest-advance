# Tests

Two layers of tests are provided.

## 1. Pure-logic unit tests (no conductor)

The rules that do not need a Holochain host live in `crates/logic`:

- exact decimal money arithmetic (`0.1 × 3 == "0.30"`, TC-20)
- campaign and batch state-transition tables (§4.1 / §4.2)
- 128-bit id formatting (32 lowercase hex chars)
- required-field / numeric / enum validation (§12)

Run:

```bash
cargo test -p harvest-advance-logic
```

These run natively and do not require `wasm32-unknown-unknown`, a conductor, or a
network.

## 2. Conductor zome-call smoke tests

`tests/conductor/smoke.mjs` drives the public zome-call surface against a real
conductor and asserts the observable behaviour of the GREEN tests and the
RED-UNTIL-FIXED items that are testable through the public API.

One-command run (starts a throwaway sandbox, installs the packed hApp, runs the
suite, stops the conductor):

```bash
# from the repository root, after building/packing
cargo build --release --target wasm32-unknown-unknown \
  -p harvest_advance_integrity -p harvest_advance
hc dna pack dna
hc app pack happ -o dist/harvest-advance.happ
./tests/conductor/run.sh
```

`run.sh` uses admin port `34817` and app port `9000`; override with
`ADMIN_PORT` / `APP_PORT` / `APP_ID` if needed. It installs the JS client
once into `tests/conductor/node_modules` (git-ignored).

The suite currently covers, among others:

| Test | Behaviour |
|---|---|
| TC-09, TC-11, TC-12, TC-13 | create_campaign rejects zero target, `end <= start`, zero goal, empty name |
| TC-14 | unknown enterprise falls back to caller ownership |
| TC-17 | Full / Installments plan fields |
| TC-18, TC-19 | pledge rejects quantity <= 0 and empty unit price |
| TC-20 | `0.1 × 3` stored as `"0.30"` |
| TC-22 | list_campaigns state and enterprise filters |
| TC-23, TC-24, TC-25, TC-26, TC-27 | batch + report creation and reads |
| TC-29 | one profile per agent |
| TC-30, TC-31, TC-53 | projection periods validation and empty lines |
| TC-32, TC-33, TC-34, TC-35, TC-36, TC-37 | campaign transitions and errors |
| TC-38 | campaign address stable across updates |
| TC-47 | list_campaigns reads the shared `campaigns` anchor |
| TC-49, TC-51 | invalid address errors; unknown address returns null |
| TC-52 | projection linked from the campaign anchor |
| TC-55 | contribution wire shape |
| TC-56 | invalid enum rejected |
| TC-58, TC-59 | the four signals fire; CampaignFunded never fires |

## 3. A/B lifecycle tests (two agents)

`tests/conductor/ab-test.mjs` runs two agents on one conductor — **A** = campaign owner,
**B** = contributor — and exercises the pledge lifecycle:

- **A (happy path):** create campaign -> pledge -> accept -> pay -> complete.
- **B (failure paths):** decline (owner rejects), fail-to-pay (default), fail-to-deliver
  (batch `Failed` + campaign `Failed`), plus authorisation negatives (non-owner cannot
  accept/recalculate; zero/overpayment rejected).

One-command run (builds + packs, starts a throwaway conductor on ports 34818/9001 so it
does not collide with `run.sh`, runs the suite, stops the conductor):

```bash
./tests/conductor/run-ab.sh
```

Override ports with `ADMIN_PORT` / `APP_PORT`.

### Tests that need a second agent / raw actions

The following cannot be exercised from a single-agent public-API harness and are
covered by design review plus the integrity unit rules, but are not automated here:

- **TC-01 / TC-04 / TC-03** — exact entry-type and link-type declaration order.
  The order is fixed in `zomes/integrity/src/entries.rs` and `links.rs`; a
  client-side check can be done by decoding the DNA manifest / `zome_info`.
- **TC-40** — parent-linkage rejection for a campaign created for another
  agent's enterprise. Requires a second agent.
- **TC-43** — ProjectionLine authorisation against another agent's projection.
  Requires a second agent.
- **TC-45 / TC-46** — self-link and wrong-link-type rejection. There is no
  public zome call that creates arbitrary links, so these need a raw
  `create_link` action (e.g. a `hc` dev action or a two-conductor test).
- **TC-48** — campaign with `end_at < start_at` is covered by the coordinator
  rule (same as TC-11); the integrity rule is exercised by the pure rule tests.

A two-conductor harness would use `hc sandbox generate -n 2` with two app
interfaces and separate agents; the same `smoke.mjs` call helper can be reused
with different `ADMIN_URL`/`APP_URL` pairs.