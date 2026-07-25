---
title: Generating SDKs
description: Generate client SDKs from your JustAPI OpenAPI schema for multiple languages.
keywords: [JustAPI, SDK generation, OpenAPI, client generation, code generation]
---

## Automatic OpenAPI Schema

JustAPI generates an OpenAPI 3.1 schema at `/openapi.json`:

```bash
curl http://localhost:8000/openapi.json
```

## Generate Python Client

```bash
pip install openapi-generator-cli

openapi-generator-cli generate \
  -i http://localhost:8000/openapi.json \
  -g python \
  -o ./clients/python
```

## Generate TypeScript Client

```bash
openapi-generator-cli generate \
  -i http://localhost:8000/openapi.json \
  -g typescript-axios \
  -o ./clients/typescript
```

## Available Languages

| Language | Generator |
|----------|-----------|
| Python | `python` |
| TypeScript | `typescript-axios` |
| Go | `go` |
| Java | `java` |
| Ruby | `ruby` |
| C# | `csharp` |
| Swift | `swift5` |

## Custom ID Function

Customize operation IDs for better SDK generation:

```python
from justapi import JustAPIApp

def custom_id(operation):
    return f"{operation.tags[0]}_{operation.name}" if operation.tags else operation.name

app = JustAPIApp(generate_unique_id_function=custom_id)
```

## See Also

- [OpenAPI Callbacks & Webhooks](/advanced/openapi-callbacks/) — outbound schemas
- [Additional Responses in OpenAPI](/advanced/additional-responses/) — error docs
- [Metadata & Docs URLs](/tutorials/metadata/) — app metadata
