# Glossary Format

Preserve an existing glossary's structure. For a new one, use:

```markdown
# Ordering

Tracks customer orders and their agreed scope.

## Language

**Order**:
A customer's request for one or more products.
_Avoid_: Purchase, transaction

**Customer**:
A person or organization that places orders.
_Avoid_: Client, buyer, account
```

Define what a concept is in one or two sentences. Pick a canonical term for synonyms; list discouraged alternatives under `_Avoid_` only when useful. Include concepts specific to this context, not generic programming terms. Group terms when natural clusters emerge.

Keep implementation choices and unresolved assumptions out of definitions. Code evidence can reveal a mismatch, but cannot by itself establish the agreed business meaning.

For multiple established contexts, follow the existing root `GLOSSARY-MAP.md`. A useful map links each context's glossary and describes domain relationships. The same word can have distinct meanings in different contexts; do not collapse them into one definition. Ask which context applies when necessary.

If neither a glossary nor map exists, create a root glossary only after resolving the first term, within authorized document scope. Add a map only when actual context boundaries justify one.
