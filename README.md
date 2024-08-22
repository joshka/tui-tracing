# Tui-tracing

An opinionated replacement for tui-logging with the goal of being something that displays nicer by
default (closer to the tracing-subscriber default view), and that makes it possible to handle event
aggregation (i.e. making events that repeat only show up once but still be something that can be
drilled into). The eventual goal is this is part of the tracing console rather than necessarily
something that would be used in each application.

There's some overlap there.

The Timing layer parts have an associated PR in tracing_subscriber. They may land there or may not
<https://github.com/tokio-rs/tracing/pull/3063>

Status: very much a WIP...

The demo example is the main driver of this. I work generally by writing code into demo and then
extracting reusable bits from the code that make sense.

Expect this not to be stable.
