import tempfile
import unittest
from types import SimpleNamespace
from pathlib import Path
from unittest.mock import patch

import doudian_fetch_payment_events as payment_events
from doudian_fetch_orders import DoudianApiError


def expired_error() -> DoudianApiError:
    return DoudianApiError(
        method="order.searchList",
        code=40003,
        msg="illegal parameter",
        sub_code="isv.access-token-expired",
        sub_msg="access_token expired",
    )


class TokenRefreshTests(unittest.TestCase):
    def request_args(self, root: Path) -> dict:
        return {
            "access_token": "old-access",
            "token_env": {"DOUYIN_OPEN_REFRESH_TOKEN": "saved-refresh"},
            "token_file": root / "doudian_token.env",
            "env_file": root / ".env",
            "sdk_path": root / "sdk",
            "create_time_start": 1,
            "create_time_end": 2,
            "order_status": "2,3,5",
            "page_size": 50,
        }

    def test_expired_access_token_refreshes_and_retries_once(self):
        with tempfile.TemporaryDirectory() as directory:
            args = self.request_args(Path(directory))
            with patch.object(
                payment_events,
                "fetch_orders",
                side_effect=[expired_error(), [{"order_id": "1"}]],
            ) as fetch_mock, patch.object(
                payment_events,
                "refresh_access_token",
                return_value="new-access",
            ) as refresh_mock:
                result = payment_events.fetch_orders_with_auto_refresh(**args)

        self.assertEqual(result, [{"order_id": "1"}])
        self.assertEqual(fetch_mock.call_count, 2)
        self.assertEqual(fetch_mock.call_args_list[0].kwargs["access_token"], "old-access")
        self.assertEqual(fetch_mock.call_args_list[1].kwargs["access_token"], "new-access")
        refresh_mock.assert_called_once()

    def test_non_token_error_is_not_refreshed(self):
        error = DoudianApiError(
            method="order.searchList",
            code=50000,
            msg="server error",
            sub_code="isv.system-error",
            sub_msg="retry later",
        )
        with tempfile.TemporaryDirectory() as directory:
            args = self.request_args(Path(directory))
            with patch.object(payment_events, "fetch_orders", side_effect=error), patch.object(
                payment_events,
                "refresh_access_token",
            ) as refresh_mock:
                with self.assertRaises(DoudianApiError):
                    payment_events.fetch_orders_with_auto_refresh(**args)

        refresh_mock.assert_not_called()

    def test_other_40003_error_is_not_misclassified_as_expired_token(self):
        error = DoudianApiError(
            method="order.searchList",
            code=40003,
            msg="illegal parameter",
            sub_code="isv.invalid-param",
            sub_msg="bad page size",
        )
        self.assertFalse(error.is_access_token_expired)

    def test_token_file_is_replaced_with_complete_fields(self):
        with tempfile.TemporaryDirectory() as directory:
            token_file = Path(directory) / "doudian_token.env"
            payment_events.write_token_env_atomic(
                token_file,
                {
                    "access_token": "new-access",
                    "refresh_token": "new-refresh",
                    "expires_in": "604800",
                    "shop_id": "123",
                    "shop_name": "test shop",
                },
            )
            content = token_file.read_text(encoding="utf-8")

        self.assertIn("DOUYIN_OPEN_ACCESS_TOKEN=new-access", content)
        self.assertIn("DOUYIN_OPEN_REFRESH_TOKEN=new-refresh", content)
        self.assertIn("DOUYIN_OPEN_SHOP_ID=123", content)

    def test_expired_refresh_token_falls_back_to_bound_shop(self):
        failed_token = SimpleNamespace(
            accessTokenResp=SimpleNamespace(sub_msg="refresh expired", msg="failed"),
            isSuccess=lambda: False,
        )
        successful_token = SimpleNamespace(
            accessTokenResp=SimpleNamespace(
                data={
                    "access_token": "renewed-access",
                    "refresh_token": "renewed-refresh",
                    "expires_in": 604800,
                    "shop_id": 123,
                    "shop_name": "test shop",
                }
            ),
            isSuccess=lambda: True,
        )

        class FakeBuilder:
            refresh_calls = []
            shop_calls = []

            @classmethod
            def refreshToken(cls, value):
                cls.refresh_calls.append(value)
                return failed_token

            @classmethod
            def buildTokenByShopId(cls, value):
                cls.shop_calls.append(value)
                return successful_token

        fake_module = SimpleNamespace(AccessTokenBuilder=FakeBuilder)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            token_file = root / "doudian_token.env"
            with patch.object(payment_events, "configure_sdk"), patch.dict(
                "sys.modules",
                {"doudian.core.AccessTokenBuilder": fake_module},
            ):
                access_token = payment_events.refresh_access_token(
                    env_file=root / ".env",
                    sdk_path=root / "sdk",
                    token_file=token_file,
                    token_env={
                        "DOUYIN_OPEN_REFRESH_TOKEN": "expired-refresh",
                        "DOUYIN_OPEN_SHOP_ID": "123",
                    },
                )

            saved = token_file.read_text(encoding="utf-8")

        self.assertEqual(access_token, "renewed-access")
        self.assertEqual(FakeBuilder.refresh_calls, ["expired-refresh"])
        self.assertEqual(FakeBuilder.shop_calls, ["123"])
        self.assertIn("DOUYIN_OPEN_REFRESH_TOKEN=renewed-refresh", saved)

    def test_missing_refresh_token_can_use_bound_shop(self):
        successful_token = SimpleNamespace(
            accessTokenResp=SimpleNamespace(
                data={
                    "access_token": "shop-access",
                    "refresh_token": "shop-refresh",
                    "shop_id": 456,
                }
            ),
            isSuccess=lambda: True,
        )

        class FakeBuilder:
            @classmethod
            def buildTokenByShopId(cls, value):
                self.assertEqual(value, "456")
                return successful_token

        fake_module = SimpleNamespace(AccessTokenBuilder=FakeBuilder)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with patch.object(payment_events, "configure_sdk"), patch.dict(
                "sys.modules",
                {"doudian.core.AccessTokenBuilder": fake_module},
            ):
                access_token = payment_events.refresh_access_token(
                    env_file=root / ".env",
                    sdk_path=root / "sdk",
                    token_file=root / "doudian_token.env",
                    token_env={"DOUYIN_OPEN_SHOP_ID": "456"},
                )

        self.assertEqual(access_token, "shop-access")


if __name__ == "__main__":
    unittest.main()
