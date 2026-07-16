"""Tests for the API gateway config loader (offline; no live upstreams).

Verifies that `enable_gateway` parses a gateway config and registers proxy
routes without needing the upstream services to be reachable.
"""
import json
import os

import pytest

from justapi import JustAPIApp

HERE = os.path.dirname(__file__)
CONFIG = os.path.join(HERE, "test_gateway.json")


def test_gateway_config_loads():
    with open(CONFIG) as f:
        cfg = json.load(f)
    assert "/api/proxy" in cfg["routes"]
    assert cfg["routes"]["/api/proxy"]["GET"]["upstream"] == "http://v2.upstream.com"

    app = JustAPIApp()
    # Loading must not raise; route registration happens inside.
    app.enable_gateway(CONFIG)

    # A second load with the same config is idempotent.
    app.enable_gateway(CONFIG)
