 # mediumcoeli — agent instructions

Project-specific rules for AI agents working in this repository.

---

Use test driven design.

Use best practices and simple architecture.

Always consult the local source code for information about Rust dependencies, which is guaranteed to
be up-to-date for the correct version.

Run `cargo path NAME` to find the source directory for a dependency.

## Starcat (`crates/pericynthion` + `crates/starcat`)

Ensure when focusing on a web endpoint, ensure all CRUD operations are well modelled or documented inline.

## Blackmoon (`crates/astrogram` + `crates/blackmoon`)

Never write to the filesystem
 - the names of astrology db specimens (possible pii leak)
 - explicit path names that are not below this point in the directory structure

README.md is a promise to the world. It needs to reflect the reality of the code base.
If this is not the case, flag it and prepare options to bring the two into convergence.

### Test corpus

Real-world test specimens are resolved from `$ASTRO_SPECIMENS` — point it at the corpus root (subdirs `sfcht/`, `zdb/`, `adb/`). Acceptance tests skip cleanly when the env var is unset.

## Roadmap

Deferred work and larger work items are captured by (superpowers) /writing-plans 
- Every deferred item **must** carry a complexity estimate.
- Use a simple three-point scale: **S** (hours), **M** (days), **L** (weeks+).
- Place the estimate at the end of the section heading line, e.g.
  `### sqlite write — M`
- When adding or updating a roadmap item, assign an estimate; do not leave it blank.

- As items from the roadmap are completed, ensure that succinct what, why and how of the work is
  captured in usage information, in `docs/` as domain-specific markdown files, documentation generator sources,
  or inline code comments, as appropriate. Check them off in the plan file as they are accomplished.

