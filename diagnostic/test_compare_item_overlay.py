"""Framework-free synthetic checks for the production rectangle comparator."""
from compare_item_overlay import metrics

same = [[(32, 32, 32)] * 2 for _ in range(2)]
different = [[(40, 32, 32)] * 2 for _ in range(2)]
assert metrics(same, same, [32, 32, 32])["rgbMAE"] == 0.0
assert metrics(same, different, [32, 32, 32])["rgbMAE"] > 0.0
assert metrics(same, different, [32, 32, 32])["neutralBackgroundIncluded"] is True
print("item overlay comparator synthetic same/different: PASS")
