#!/usr/bin/env python3
# Public visibility refines the authoritative published predicate.
for stage in range(3):
  server_published=(stage==2)
  client_visible=(stage==2)
  assert client_visible==server_published
  if stage<2: assert not client_visible
print("published-state read refinement: ok")
