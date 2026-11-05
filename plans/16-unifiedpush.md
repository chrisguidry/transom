# 16, UnifiedPush

Not built. Stub.

Delivering to phones through a UnifiedPush distributor, such as ntfy,
as a third choice beside Google Play services and the direct
connection.

## Open

- transom already sends Web Push, encrypted per RFC 8291, and
  UnifiedPush's Android connector receives Web Push, so a UnifiedPush
  device may be a Web Push device with the distributor's endpoint.
  Whether the app needs more than that, and what Settings offers.
- Whether Google Play services delivery moves onto UnifiedPush too,
  through an FCM distributor and the gateway, so the gateway sees only
  ciphertext.
