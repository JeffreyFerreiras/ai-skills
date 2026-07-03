---
name: api-docs
description: Add XML documentation comments to code files. Use when the user wants to document public APIs, classes, methods, or properties with accurate, descriptive XML doc comments. Checks how code is used in the codebase for accuracy and uses inheritdoc for interface implementations.
---

# API Documentation Prompt

Add XML documentation to the code files. The documentation should be descriptive and clearly define the purpose of each attribute or method. Check for how the code is used in other parts of the codebase to ensure accuracy.

For classes that implement interfaces:
- Use `<inheritdoc />` in the class documentation
- Place the detailed comments in the interface definition

Ensure all public members have appropriate XML documentation comments.
