---
name: api-docs
description: Add XML documentation comments to code files. Use when the user wants to document public APIs, classes, methods, or properties with accurate, descriptive XML doc comments. Checks how code is used in the codebase for accuracy and uses inheritdoc for interface implementations.
---

# API Documentation Prompt

Add XML documentation to the code files. The documentation should be descriptive and clearly define the purpose of each attribute or method. Check for how the code is used in other parts of the codebase to ensure accuracy. The comments should provide business context and be understandable to other developers. Use `<inheritdoc />` for classes that implement interfaces, placing detailed comments in the interface definition. Ensure all public members have appropriate XML documentation comments. By default, apply to the changes in this PR, diffed from master unless otherwise specified. If the user requests, apply to all public members in the codebase.

## .NET XML Documentation Guidelines
For classes that implement interfaces:

- Use `<inheritdoc />` in the class documentation
- Place the detailed comments in the interface definition

Ensure all public members have appropriate XML documentation comments.
