---
name: clean-architecture-code
description: Implement or refactor code across domain, application, and infrastructure boundaries. Use when a change needs Clean Architecture or dependency inversion.
---

# Clean Architecture Code

## Overview

Use Clean Architecture as a constraint on implementation, not as ceremony. Keep business rules independent, let application use cases coordinate workflows, and push frameworks, databases, UI, network, filesystem, and SDK details outward.

Use this workflow when the change involves architectural boundaries. For a local naming or readability refactor without boundary changes, use `clean-code`; for review without edits, use `clean-architecture-review`.

General naming, cohesion, abstraction choices, SOLID guidance, and change validation belong to `clean-code`. This skill adds the layer and boundary rules below.

## Workflow

1. Identify the project's policy and mechanism boundaries. The four circles are schematic, not a required folder or layer count; preserve the dependency rule across the boundaries the project needs.
2. Name the policy being added or changed. Decide whether it belongs in domain rules, an application use case, an adapter, infrastructure, or composition.
3. Preserve the source dependency rule. Inner code must not name outer declarations in imports, signatures, inheritance, annotations, or data formats. Runtime control may flow outward through an inner-owned port implemented outward.
4. Keep use-case input and output data isolated and simple, in the form most convenient for the inner policy. Map entities and database rows into boundary data instead of passing them through input/output ports. Do not pass framework requests, SDK models, UI components, or database objects inward.
5. Make domain rules and use cases testable without a UI, database, web server, or external service. Adapter tests cover translation and integration separately; no particular test-writing order is required.

## Layer Guide

- Domain/entities: the most general enterprise or application business rules and invariants. These may be objects or data structures and functions; they need no particular object model. No framework, database, HTTP, filesystem, LLM SDK, UI, or environment imports.
- Application/use cases: application-specific business rules and workflows. Orchestrate domain rules, transaction boundaries, ports, DTOs, and application-specific authorization/policy checks. Changes to application workflows should not force changes to general domain rules.
- Interface adapters: controllers, presenters, mappers, gateways, view models, serializers, and repository adapters. Translate between external formats and application/domain models.
- Infrastructure/frameworks: database clients, web framework wiring, SDK clients, filesystem/network access, queues, runtime configuration, and dependency injection composition.

## Implementation Rules

- Define ports close to the use case that owns the need. Let outer adapters depend on those ports.
- Express persistence ports in terms of the inner policy's needs. Keep SQL, ORM queries, and database row formats in outer persistence adapters; aggregate-based repositories are an option, not a Clean Architecture requirement.
- Put mapping at boundaries. Do not let ORM entities, JSON payloads, React props, FastAPI/Express request objects, or SDK response types become domain objects.
- Compose dependencies at the outermost application entry point.
- Keep technical failure mapping and any required retry/timeout mechanics at the adapter boundary; application-specific failure decisions remain in the use case.
- Refactor toward boundaries when framework or persistence concerns are already leaking into business rules.

## Boundary Patterns

Use this pattern when an inner use case must trigger an outer behavior:

```text
application/use-case defines OutputPort or GatewayPort
adapter implements that port
infrastructure wires implementation into the use case
```

Runtime control can flow outward while source dependencies still point inward. A use case calls its own output port, not a concrete presenter. Examples of inward source dependencies:

```text
controller -> use case -> domain
repository implementation -> application repository port
presenter -> application output DTO
```

## Before Finishing

- Verify inner layers have no references to outer declarations or outer-owned data formats.
- Verify use cases depend on abstractions for side effects.
- Verify DTOs or simple data cross boundaries.
- Verify changed domain rules and use cases can be tested without external systems.
- Check that replacing the UI, database, or framework would leave business rules unchanged; this is a dependency assessment, not a requirement to implement a second adapter.
- State any intentional boundary compromise and why it is acceptable.

## Use-Case-Centered Structure

Organize architectural modules around the system's use cases rather than letting a web or persistence framework dictate the structure. This concerns architectural organization, not general identifier naming. Keep delivery and storage choices peripheral so they can be deferred or replaced without rewriting business rules; do not replace established technology merely to demonstrate independence.

## Sources

- Robert C. Martin, [The Clean Architecture](https://blog.cleancoder.com/uncle-bob/2012/08/13/the-clean-architecture.html): dependency rule, policy layers, boundary control and data, and independence from external details.
- Robert C. Martin, [Screaming Architecture](https://blog.cleancoder.com/uncle-bob/2011/09/30/Screaming-Architecture.html): use-case-centered structure, deferred technology decisions, and independently testable business rules.
