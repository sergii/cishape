# Product and architecture insights

This directory captures **CIShape-specific product/architecture insights that we want to preserve across implementation cycles**, but which are not necessarily implemented yet.

It is intentionally different from:

- `docs/research/` - prior art and external ideas with no CIShape decision implied;
- `docs/rfc/` - concrete proposals ready for design/implementation review;
- `docs/roadmap.md` - committed or sequenced delivery work;
- ADRs - durable implementation decisions.

An insight snapshot may contain a mixture of:

- agreed product direction;
- terminology we want to keep stable;
- architecture boundaries;
- UX principles;
- hypotheses that still need validation;
- candidate implementation slices;
- links to relevant market/prior-art references.

Every insight note should clearly mark which statements are **direction**, **current implementation**, or **open question** so future work does not accidentally treat discussion as shipped behavior.

## Insight snapshots

- [Continuous CI profiler and advisor](continuous-ci-profiler-advisor.md)
- [Local-first, build-model-native, and agent-native CI](local-first-and-build-model-native-ci.md) - NixCI and Preloop prior art, CI market taxonomy, feedback-loop metrics, and benchmark hypotheses.
