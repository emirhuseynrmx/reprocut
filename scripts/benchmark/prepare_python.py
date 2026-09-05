#!/usr/bin/env python3
"""Build the realistic Python benchmark case for the multi-reducer arena."""

from __future__ import annotations

import sys
from pathlib import Path

SAMPLE_CODE = """"""Realistic data processing and invoice calculation module with realistic dead baggage."""
from __future__ import annotations

import datetime
import json
import math
import sys
from decimal import Decimal
from typing import Any, Dict, List, Optional


class CustomerProfile:
    """Customer account representation (dead baggage)."""
    def __init__(self, customer_id: str, email: str, tier: str = "standard"):
        self.customer_id = customer_id
        self.email = email
        self.tier = tier

    def formatted_badge(self) -> str:
        return f"[{self.tier.upper()}] {self.email}"


class DiscountEngine:
    """Tiered discount computation helper (dead baggage)."""
    def __init__(self, promo_code: Optional[str] = None):
        self.promo_code = promo_code

    def calculate_rebate(self, subtotal: Decimal) -> Decimal:
        if self.promo_code == "VIP50":
            return subtotal * Decimal("0.50")
        if self.promo_code == "SPRING10":
            return subtotal * Decimal("0.10")
        return Decimal("0.00")


def format_currency_output(amount: Decimal, symbol: str = "$") -> str:
    """Format numeric amount as localized currency string (dead baggage)."""
    rounded = round(float(amount), 2)
    return f"{symbol}{rounded:,.2f}"


def calculate_applicable_sales_tax(subtotal: Decimal, state: str) -> Decimal:
    """Multi-branch sales tax lookup (dead baggage)."""
    rates = {"CA": Decimal("0.0825"), "NY": Decimal("0.0800"), "TX": Decimal("0.0625")}
    rate = rates.get(state, Decimal("0.0500"))
    return subtotal * rate


def process_order_batch(records: List[Dict[str, Any]]) -> List[str]:
    """Batch pipeline processor (dead baggage)."""
    summaries = []
    for record in records:
        customer = CustomerProfile(record["user"], record.get("email", "unknown"))
        summaries.append(f"{customer.formatted_badge()}: {record.get('status', 'PENDING')}")
    return summaries


def compute_final_invoice(order_data: Dict[str, Any]) -> Decimal:
    """Compute total invoice amount.

    INJECTED DEFECT: shipping_fee is provided as an unparsed str rather than Decimal,
    triggering a deterministic TypeError in Python runtime.
    """
    base_subtotal = Decimal(str(order_data["subtotal"]))
    shipping_charge = order_data["shipping_fee"]  # String!
    # Injected failure site:
    final_total = base_subtotal + shipping_charge
    return final_total


if __name__ == "__main__":
    payload = {
        "invoice_id": "INV-2026-9811",
        "subtotal": "149.95",
        "shipping_fee": "12.50",
    }
    compute_final_invoice(payload)
"""


def main():
    if len(sys.argv) < 2:
        print("Usage: python prepare_python.py <workdir>")
        return 1
    work = Path(sys.argv[1]).resolve()
    work.mkdir(parents=True, exist_ok=True)
    target = work / "original.py"
    target.write_text(SAMPLE_CODE, encoding="utf-8")
    print(f"Prepared Python benchmark case at {target} ({len(SAMPLE_CODE)} bytes, {len(SAMPLE_CODE.splitlines())} lines)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
