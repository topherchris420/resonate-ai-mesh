# Mathematical research agenda: Pordenone / Resonate AI Mesh

**Status:** candidate research protocol, not an implemented algorithmic improvement or a new empirical result.

## Why this exists

[OpenAI's `math`](https://github.com/openai/math) is a mathematics manuscript collection with some accompanying Lean proof artifacts, not an installable mathematics or inference engine. Its own [README](https://github.com/openai/math/blob/main/README.md) warns that some manuscripts do not have formalizations and some unformalized results may have issues.

The current R.A.I.N. Lab at [Lop Nur Twin](https://github.com/topherchris420/lop-nur-twin) already holds a commit-pinned, read-only lexical index derived from that collection. **Use one shared substrate, not eight forked copies.** Its new [portfolio scout](https://github.com/topherchris420/lop-nur-twin/blob/main/docs/MATH_PORTFOLIO.md) can return candidate titles, matched terms, upstream commit and index hash, without upgrading source material to proven product behavior.

The upstream catalogue was inspected at commit [`fd4aeeb2ee4f`](https://github.com/openai/math/tree/fd4aeeb2ee4fc729c18d98444fed42fd0529eeeb) on 9 October 2026. **The bundled R.A.I.N. index may pin a different commit**; its reported provenance, not the inspection date, controls the scouting results.

## The question

**Do authorization invariants survive stale observations and competing agent proposals?**

Candidate search terms: `graph routing constraints bounded optimization`

From a checkout of [R.A.I.N. Lab](https://github.com/topherchris420/lop-nur-twin):

```sh
npm run rain:math:verify
npm run rain:math:portfolio -- --project resonate-ai-mesh --json
```

An empty result is acceptable. Search hits are lexical references; they are not recommendations to transfer a theorem, and a source's Lean listing is not proof of a new experimental finding in this repository.

## Assumptions that must be mapped before importing a mathematical idea

1. The validator receives a complete, correctly time-stamped view of each simulated proposal.
2. State writes occur only through the authorized commit path.
3. The compared policies see the same seeded disturbances.

## Baseline already available

`make demo` to explain a blocked decision; `make check` to validate code, deterministic replay and the existing test suite.

## Next falsifiable experiment (not yet executed)

Use the existing perturbed-mesh scenario, replay and paired fault-injection experiments. For the next study, preregister a boundary perturbation that makes one observation stale exactly at the point of human approval. Measure unintended commits, task completion and decision holds across the same seeds.

**Negative control:** Remove or disable exactly one relevant deterministic gate **only in an isolated research branch**, preserving the rest. It should change the corresponding failure count while leaving unaffected cases comparable.

**Completion artifact:** A new experiment manifest, paired run summaries, the failing-case trace and a version-pinned comparison, with unchanged authorization code in production.

## Authority, scope and refusal

A matching paper title about graph routing cannot prove this Rust policy gate safe in real deployments.

Do not change a control policy, a research registry verdict, an experiment's preregistered criteria, the deployed product, or the meaning of an evidence class on the strength of a matched mathematical phrase. A formal proof establishes only the theorem and assumptions actually formalized; a software simulation tests its own implementation and scenarios; external and physical claims require separate evidence.

**Current verdict:** `NOT TESTED` for this proposed mathematical extension. This document is a reproducible research question and a reference-workflow entry, not a passing benchmark.

See also [the R.A.I.N. mathematics workflow](https://github.com/topherchris420/lop-nur-twin/blob/main/docs/MATH_PORTFOLIO.md) and the [OpenAI mathematics catalogue](https://github.com/openai/math/blob/fd4aeeb2ee4fc729c18d98444fed42fd0529eeeb/CONTENTS.md).
