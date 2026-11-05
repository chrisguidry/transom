# 17, Replicas

Not built. Stub.

The transom server must support multiple replicas for high
availability.

## Open

- What the server keeps in memory per process today, such as each
  live feed's subscribers and the once-a-second pass, and what of it
  has to move to Redis or be safe to run on every replica at once.
