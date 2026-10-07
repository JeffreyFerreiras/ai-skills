---
name: clean-architecture-review
description: Review architecture plans or code for dependency direction, layer separation, and boundary leaks. Use for Clean Architecture reviews.
---

# Clean Architecture Review

## Overview

Review dependency direction, boundary leakage, misplaced business rules, and port ownership.

Use this focused workflow for architecture questions. For a general diff or regression review, use `clean-code-review` and inspect architectural concerns only where the evidence warrants them. Review requests do not authorize edits.

General review coverage, SOLID checks, abstraction choices, evidence thresholds, severity, and findings format belong to `clean-code-review`. This skill adds the architecture checks below; use the general review skill's severity and output guidance for architecture findings too.

## Review Workflow

1. Map the code under review to the project's actual layers. Use the local naming, but classify responsibilities as domain, application/use case, interface adapter, infrastructure/framework, and composition.
2. Trace source dependencies through imports, signatures, inheritance, annotations, data formats, and configuration wiring. Trace runtime control separately: outward execution through an inner-owned port does not itself violate the dependency rule.
3. Inspect use-case input and output boundaries. Verify that entities, ORM rows, framework objects, SDK responses, UI props, and transport payloads are mapped into isolated data suited to the inner policy.
4. Locate business rules. Flag rules hidden in controllers, persistence adapters, UI components, migrations, scripts, or SDK wrappers when they belong in domain or application code.
5. Check whether domain rules and use cases can be tested without a UI, database, web server, or external service. Adapter tests verify mapping, integration contracts, and technical failure translation; application-specific failure policy belongs inward.

## Checks

Dependency rule:
- The rule covers all references to outer declarations and formats, not just imports. Runtime control direction is distinct from source dependency direction.
- Domain must not import application, adapters, infrastructure, UI, framework, database, filesystem, network, or SDK code.
- Application may depend on domain and application-owned ports/DTOs, not adapter implementations.
- Adapters may depend inward and translate data both ways.
- Infrastructure and app composition wire concrete implementations at the edge.

Boundary data:
- Prefer isolated DTOs, value objects, commands, queries, or simple arguments shaped for the inner policy, without outer dependencies.
- Flag framework request/response objects, ORM entities, SDK models, or UI state passed into inner layers.
- Flag entities or database rows passed through use-case input/output ports instead of mapped boundary data. Use cases may still work with entities internally.

Ports and dependency inversion:
- Use ports when an inner layer needs persistence, time, IDs, external APIs, messaging, presentation, or side effects.
- Ports belong to the inner policy that owns the need. Keep SQL, ORM queries, and database row formats in outer persistence adapters; do not require aggregate-based repositories.
- Implementations belong outward and are injected through composition.
- A use case that invokes a presenter must call an inner-owned output port rather than depend on the concrete presenter.

Layer responsibility:
- Domain owns the most general business rules and invariants; objects are not required where data structures and functions serve the same role.
- Application owns application-specific business rules and workflow orchestration. Workflow changes should not force changes to general domain rules.
- Adapters own translation.
- Infrastructure owns technical details.
- Composition owns construction and wiring.

Architectural independence:
- Check whether replacing a UI, database, or framework would require business-rule changes. Assess dependency paths; do not require an actual replacement implementation.
- Check that architectural modules express the system's use cases rather than framework structure. Flag framework-driven organization when it obscures use cases or couples policy to delivery/storage details, not solely because of folder names.
- For architecture plans, check that use cases can be specified without prematurely binding them to a framework, database, or delivery mechanism. Existing technology choices do not themselves violate independence.

## Pragmatic Limits

- The four circles are schematic. Do not require exactly four folders or layers; review the dependency rule across the actual policy boundaries.
- Do flag simple code when it puts policy inside volatile frameworks or makes core rules hard to test.

## Sources

- Robert C. Martin, [The Clean Architecture](https://blog.cleancoder.com/uncle-bob/2012/08/13/the-clean-architecture.html): dependency rule, policy layers, boundary control and data, and independence from external details.
- Robert C. Martin, [Screaming Architecture](https://blog.cleancoder.com/uncle-bob/2011/09/30/Screaming-Architecture.html): use-case-centered structure, deferred technology decisions, and independently testable business rules.
