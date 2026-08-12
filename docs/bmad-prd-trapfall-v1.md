# TrapFall PRD — Product Requirements Document
> BMAD Brownfield Analysis | Date: 2026-08-12
> Status: ✅ FINAL — All 5 C-Level Perspectives Synthesized
> Analyst: CTO | Discussion: 5-C-Level Multi-Agent via Uteke Coordination

---

## 1. Executive Summary

TrapFall is a self-hosted, Sentry SDK-compatible error capture engine in Rust (Axum) + SvelteKit 5. Apache-2.0. **Only Rust-based error tracker in existence.** Unique: MCP-first (12 tools), 6MB Docker image, dual SQLite/Postgres.

**Fused Verdict: CONDITIONAL GO — Ecosystem component, not standalone product.**
**Fused Score: 5.3/10** (CTO 7 + CFO 2 + CMO 6.5 + COO 7 + CLO 6.5 risk = 29/5.5)

---

## 2. Ground Truth (All Verified)

| Metric | Value | Source |
|--------|-------|--------|
| Rust LOC | ~9,900 (9 crates) | CTO |
| TypeScript LOC | ~4,164 (11 pages) | CTO |
| Tests | **231** (not 101 — COO correction) | COO |
| MCP Tools | 12 (production-ready) | CTO |
| Docker Image | 5.75MB (scratch+MUSL) | CTO |
| CI/CD | 6 workflows, **9/10 quality** | COO |
| Alert Engine | 266 LOC in trapfalld (NOT in alert crate) | COO |
| Version | v0.2.1 | — |
| External Users | 0 | — |
| GitHub Stars | 4 | — |
| VitePress Docs | 13 guide pages | COO |
| Postgres testcontainers | ✅ In CI | COO |

### Ghost Crates (COO Discovery)
| Crate | LOC | Reality | Action |
|-------|-----|---------|--------|
| trapfall-alert | 3 | `// TODO scaffold only`. Real impl: trapfalld/src/alert.rs (266 LOC) | DELETE |
| trapfall-dashboard | 3 | `// TODO scaffold only`. Real impl: trapfalld/src/spa.rs (62 LOC) | DELETE |
| trapfall-search | 40 | Thin pass-through to db. Uses LIKE not FTS5. | MERGE into trapfalld |

---

## 3. Five-Perspective Verdicts

### CTO: 7/10 — CONDITIONAL GO
- Architecture genuinely solid, crate boundaries (the real ones) map to domain
- 3 P0 fixes: body size limit (DoS), stub crates, batch insert pipeline
- Scalability ceiling: ~100-500 events/sec (SQLite 4 conns, no batch)
- MCP: production-ready, genuine differentiator
- Strategic: don't chase Sentry parity, double down on MCP + Rungu ecosystem

### CFO: 2/10 — NO-GO STANDALONE
- No moat, no cloud tier, TAM too small ($200-500K total market)
- Opportunity cost: every hour on TrapFall = hour NOT on Cora Code/Uteke
- Kill criteria: no 100+ stars in 6 months → stop
- Only path: TrapFall as component of Cora Code/Uteke bundle
- Priority rank: Cora Code (1) > Uteke (2) > Titen (3) > Rungu (4) > TrapFall (5)

### CMO: 6.5/10 — LAUNCH within 2 weeks
- Ideal user: privacy-conscious self-hosters + AI-native dev teams
- Positioning: "Self-hosted error tracking that your AI agent can debug"
- DSN compatibility = Trojan Horse (zero code change migration)
- Distribution: Show HN + r/selfhosted + r/rust → 100 stars in 4-6 weeks
- vs GlitchTip: "What comes AFTER Sentry — for the AI agent era"

### COO: 7/10 — RESCOPE
- Tests: 231 (not 101), distribution healthy, Postgres testcontainers in CI
- CI/CD: 9/10, best-in-class for solo project
- Ghost crates: organizational fiction, misleading
- v0.3.0 = "Honest Architecture" cleanup release (2-3 focused days)
- Merge 9→6 crates BEFORE adding features
- Breaking point: 2 more features without cleanup = unmaintainable

### CLO: 6.5/10 RISK — 2 CRITICAL BLOCKERS
| # | Blocker | Severity | Action |
|---|---------|----------|--------|
| B1 | No PII detection/redaction on ingest | CRITICAL | Server-side PII scrubbing, IP anonymization |
| B2 | No SSRF protection on webhook delivery | CRITICAL | IP blocklist, HTTPS-only, DNS rebinding mitigation |
| B3 | No data subject access/erasure | HIGH | Export/delete endpoints for UU PDP compliance |
| B4 | No data retention policy | HIGH | 30-day default, configurable, auto-purge |
| B5 | MCP returns raw PII to LLMs | CONDITIONAL | PII redaction before MCP response |

**License:** Stay Apache-2.0 (adoption priority). Reconsider AGPL if cloud tier planned.

---

## 4. Fused Strategy

### Consensus Points (5/5 agree)
1. **No cloud tier. Period.** — CFO killed it, CLO confirmed compliance gap, CTO agrees
2. **TrapFall = ecosystem component** — not standalone product (CTO+CFO agree)
3. **MCP is the ONLY defensible differentiator** — unanimous
4. **Ghost crates must be cleaned up** — CTO found them, COO confirmed, must fix
5. **Stay Apache-2.0** — CLO confirms, adoption priority

### Resolution Matrix
| Tension | CTO | CFO | CMO | COO | CLO | Resolution |
|---------|-----|-----|-----|-----|-----|-----------|
| Invest more? | Yes (P0 fixes) | No | Yes (launch) | Rescope first | Fix blockers | **1-week hardening → launch** |
| Feature parity? | No | N/A | No | No | N/A | **MCP + ecosystem, not Sentry clone** |
| When to launch? | After P0 | N/A | 2 weeks | After cleanup | After B1+B2 | **2 weeks (fix + cleanup + launch)** |
| Priority rank? | P1 | P5 (lowest) | Launch now | Rescope | Fix first | **P2 ecosystem (after Cora Code)** |

---

## 5. v0.3.0 Roadmap — "Honest Architecture"

### Week 1: Cleanup + Security (3-4 focused days)
| # | Task | Effort | Source |
|---|------|--------|--------|
| 1 | DELETE trapfall-alert crate | 0.5 day | CTO+COO |
| 2 | DELETE trapfall-dashboard crate | 0.5 day | CTO+COO |
| 3 | MERGE trapfall-search into trapfalld | 0.5 day | CTO+COO |
| 4 | Body size limit on ingest endpoint | 0.5 day | CTO |
| 5 | PII scrubbing on ingest (B1) | 1 day | CLO |
| 6 | SSRF protection on webhook delivery (B2) | 1 day | CLO |
| 7 | Data retention config + auto-purge (B4) | 1 day | CLO |
| 8 | Add inline unit tests for server.rs handlers | 1 day | COO |
| 9 | Document alert system in ARCHITECTURE.md | 0.5 day | COO |

### Week 2: Launch Prep
| # | Task | Effort | Source |
|---|------|--------|--------|
| 10 | Verify Sentry SDK compat (JS + Python real test) | 1 day | CMO |
| 11 | README polish + demo GIF | 1 day | CMO |
| 12 | Migration guide: Sentry → TrapFall (3-step GIF) | 1 day | CMO |
| 13 | Live demo instance (read-only) | 0.5 day | CMO |
| 14 | Comparison pages (vs Sentry, vs GlitchTip) | 1 day | CMO |
| 15 | Show HN + r/selfhosted + r/rust launch | 0.5 day | CMO |

### Deferred (v0.4.0+)
- Email + Slack alert channels (CTO P1, but ecosystem first)
- Batch insert pipeline (CTO P0, but current perf acceptable for target users)
- Org/team model (multi-week, needs consolidation first)
- Source map upload (JS production parity)
- SSO (OIDC only, 4-6 weekends)

---

## 6. Kill Criteria (CFO + Consensus)

Stop investing in TrapFall beyond maintenance if ANY:
- [ ] No 100+ GitHub stars within 6 months of active marketing
- [ ] No 10+ self-hosted deployments confirmed within 90 days
- [ ] No integration interest from 2+ external teams
- [ ] Star growth < 2x over next quarter despite promotion

**Current status: 4 stars, 0 users — at kill threshold unless deliberate launch push.**

---

## 7. Ecosystem Integration

```
TrapFall (errors) ←MCP→ Rungu (feedback) ←MCP→ Cora Code (review)
    ↑                    ↑                      ↑
    Sentry SDK          Webhooks              Git hooks
    (any language)      (Slack/Discord/n8n)   (pre-commit/CI)
```

**TrapFall's role:** Capture errors → MCP lets AI agents query them → Rungu feedback board links user reports to error traces → Cora Code reviews fixes with error context.

**The loop no competitor offers:**
1. App crashes → TrapFall captures (Sentry SDK, zero config)
2. TrapFall alert → Rungu auto-creates feedback post
3. Dev: "Claude, what happened?" → MCP queries both TrapFall + Rungu
4. Claude reviews the fix → Cora Code validates

---

## 8. Artifacts

- BMAD Synthesis: `docs/bmad-prd-trapfall-v1.md` (this file)
- Ecosystem Vision: `../../ecosystem-vision.md`
- Uteke room: `disc:rungu-optimization` (all rounds stored)
- Multi-agent cache: `~/profiles/cto/cache/delegation/` (5 subagent reports)
