---
title: GraphQL Integration
description: Set up Strawberry GraphQL alongside REST endpoints in a JustAPI application.
keywords: [JustAPI, GraphQL, Strawberry, REST, multi-protocol]
---

JustAPI supports running GraphQL alongside REST endpoints in the same application.

## Basic Setup

Install Strawberry:

```bash
pip install strawberry-graphql
```

Create your GraphQL schema:

```python
import strawberry
from justapi import JustAPIApp

@strawberry.type
class User:
    id: int
    name: str
    email: str

@strawberry.type
class Query:
    @strawberry.field
    def users(self) -> list[User]:
        return [User(id=1, name="Alice", email="alice@example.com")]

@strawberry.type
class Mutation:
    @strawberry.mutation
    def create_user(self, name: str, email: str) -> User:
        return User(id=2, name=name, email=email)
```

Mount the GraphQL endpoint in your JustAPI app:

```python
import strawberry
from strawberry.fastapi import GraphQLRouter

app = JustAPIApp()

schema = strawberry.Schema(query=Query, mutation=Mutation)
graphql_app = GraphQLRouter(schema)

app.include_router(graphql_app, prefix="/graphql")
```

## Testing GraphQL

Start your app and visit `http://localhost:8000/graphql` for the GraphiQL interactive explorer.

```graphql
query {
  users {
    id
    name
    email
  }
}
```

## Shared Authentication

JustAPI supports global dependencies that apply to all routes including GraphQL:

```python
from justapi import Depends, Security

def get_current_user(request):
    token = request.headers.get("Authorization", "").replace("Bearer ", "")
    # validate token...
    return {"user_id": 1, "name": "Alice"}

app = JustAPIApp(dependencies=[Depends(get_current_user)])
```

## Recap

- Install `strawberry-graphql` for GraphQL support.
- Mount `GraphQLRouter` with a prefix using `app.include_router()`.
- GraphQL runs alongside REST — no separate server needed.
- Use JustAPI's dependency injection for shared auth.

## See Also

- [Routing & Sub-routers](/tutorials/routing-subrouters/) — multiple routers
- [Security](/tutorials/security/first-steps/) — authentication
