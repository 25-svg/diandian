import unittest

from doudian_fetch_orders import normalize_order_status


class OrderStatusFilterTests(unittest.TestCase):
    def test_blank_status_disables_the_api_status_filter(self):
        self.assertIsNone(normalize_order_status("   "))

    def test_explicit_status_is_preserved(self):
        self.assertEqual(normalize_order_status("2"), "2")


if __name__ == "__main__":
    unittest.main()
