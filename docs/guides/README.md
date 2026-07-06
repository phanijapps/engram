# Engram Documentation Guides

This directory contains user-facing guides organized by the [Diátaxis framework](https://diataxis.fr/).

## Guide Types

- **Tutorials** — Hands-on, step-by-step lessons for beginners
- **How-to guides** — Task-specific recipes for competent users  
- **Reference** — Authoritative technical specifications
- **Explanation** — Conceptual background and context

## Available Guides

### Tutorials
- [Add memory to a Rust agent with Engram](tutorials/use-engram-as-memory-layer.md) — Getting started: configure, bootstrap, write, and retrieve memories, with an Ollama embeddings configuration step

### Explanations
- [How repos get indexed](explanation/how-repos-get-indexed.md) — Understanding Engram's code repository indexing

## Conventions

- **Present tense** — All guides describe current behavior ("Engram stores", not "Engram will store")
- **Code examples** — All code samples are tested and runnable
- **Error handling** — Common errors and solutions are documented
- **Cross-references** — Related guides are linked for deeper learning

## Contributing

When adding new guides:
1. Choose the correct quadrant (tutorial/how-to/reference/explanation)
2. Follow the [Diátaxis principles](https://diataxis.fr/)
3. Update this README with your new guide
4. Cross-link from related guides