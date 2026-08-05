# Order status coverage implementation plan

**Goal:** Do not silently exclude paid live orders because their current fulfillment status is no longer pending shipment.

### Task 1: Make the API status filter optional

- [ ] Add a failing Python unit test proving a blank status produces no `combine_status` value.
- [ ] Change `scripts/doudian_fetch_orders.py` so `--order-status` defaults to blank and assigns `combine_status` only for a nonblank value.
- [ ] Record `combine_status: null` in the JSON query metadata when no API status filter was applied.
- [ ] Change `scripts/doudian_fetch_orders.ps1` so it forwards `--order-status` only when explicitly supplied.
- [ ] Run the unit test and Python syntax checks.

### Task 2: Verify the caller contract

- [ ] Confirm the PowerShell wrapper exposes an empty default status and preserves explicit status values.
- [ ] Leave payment-time filtering unchanged: it remains the authoritative per-live-session filter.
