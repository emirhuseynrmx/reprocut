from noise.inventory import reserve


def test_reservation_shape() -> None:
    assert reserve("SKU-1", 2) == ("SKU-1", 2)
