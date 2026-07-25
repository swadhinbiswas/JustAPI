---
title: Mailer
description: Send emails from JustAPI applications — sync, async, and template-based.
keywords: [JustAPI, mailer, email, SMTP, send email, templates]
---

`Mailer` sends emails over SMTP. It supports synchronous and asynchronous sending, plus Jinja2 templates.

## Setup

```python
from justapi import JustAPIApp, Mailer

app = JustAPIApp()

mailer = Mailer(
    host="smtp.example.com",
    port=587,
    username="user@example.com",
    password="secret",
    use_tls=True,
    default_from="noreply@example.com",
    default_from_name="My App",
)
```

## Send Email

```python
@app.post("/send")
def send_email():
    mailer.send(
        to="recipient@example.com",
        subject="Hello",
        body="This is the plain text body",
        html="<h1>Hello</h1><p>This is the HTML body</p>",
    )
    return {"status": "sent"}
```

## Async Send

```python
@app.post("/send-async")
async def send_email_async(background_tasks: BackgroundTasks):
    background_tasks.add_task(
        mailer.send_async,
        to="recipient@example.com",
        subject="Hello",
        body="Sent asynchronously",
    )
    return {"status": "queued"}
```

## Send with Template

```python
mailer = Mailer(
    host="smtp.example.com",
    port=587,
    templates="templates/email",
)

@app.post("/send-template")
def send_welcome():
    mailer.send_template(
        template_name="welcome.html",
        to="user@example.com",
        subject="Welcome!",
        context={"name": "Alice"},
    )
    return {"status": "sent"}
```

## Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `host` | `str` | — | SMTP server hostname |
| `port` | `int` | `587` | SMTP port |
| `username` | `str` | `None` | Auth username |
| `password` | `str` | `None` | Auth password |
| `use_tls` | `bool` | `True` | Enable TLS |
| `default_from` | `str` | `None` | Default sender email |
| `default_from_name` | `str` | `None` | Default sender name |
| `templates` | `str` | `None` | Directory for Jinja2 email templates |
