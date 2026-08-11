"""Checkout calculation containing the demonstration bug."""

from __future__ import annotations

from decimal import Decimal
from typing import Any


def quote_total(order: dict[str, Any]) -> Decimal:
    subtotal = Decimal(order["subtotal"])
    currency = order["currency"]
    return subtotal + currency
