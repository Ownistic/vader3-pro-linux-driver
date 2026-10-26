# Project instructions

## Language

Write and respond in **English only**. This applies to chat replies, code
comments, documentation, commit messages, and test names. Do not reply in, or
partially in, another language even if earlier messages did.

## Hardware experiments

The owner has authorized active experiments on the physical controller,
including commands that may disconnect or reset it. Disconnects are acceptable
and are not a reason to stop.

When a disconnect happens, or whenever a step needs the owner to act (reconnect
a device, grant node access, press a button, confirm a felt result), **stop and
ask** — use the `question` tool rather than guessing or continuing blindly.

Never present a successful write call as proof that the device acted on it.
A write returning success means the transport accepted the bytes, nothing more.
Only an observed device response counts as evidence, and state clearly when a
result is unverified.

## Evidence discipline

Record which revision and which artifact were actually tested. Keep
descriptor-level, code-level, and hardware-level findings distinct: a fact
proven by static analysis is not a fact proven on the device.
