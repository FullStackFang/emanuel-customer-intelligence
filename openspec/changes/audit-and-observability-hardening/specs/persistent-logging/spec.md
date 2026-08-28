## ADDED Requirements

### Requirement: Logs are persisted to a rotating file

The application SHALL write its `tracing` output to a rotating log file under the
application data directory in addition to standard output, so diagnostics survive
process exit. The existing standard-output logging, the default log level, and the
`RUST_LOG` / environment-filter override SHALL continue to work unchanged.

#### Scenario: Log output survives process exit
- **WHEN** the application runs and later exits
- **THEN** a log file under the application data directory contains the log events emitted during the run

#### Scenario: Standard output logging is preserved
- **WHEN** the application is launched from a console
- **THEN** log events still appear on standard output as before, in addition to the file

#### Scenario: Environment filter still applies
- **WHEN** the log level is overridden through the environment filter
- **THEN** both the file and standard-output sinks honor the overridden level

### Requirement: Each completed chat turn emits a telemetry event

On completion of a chat turn that is not cancelled, the application SHALL emit one
structured log event recording the backend used, the elapsed time in milliseconds, and
the token counts when the backend reports them. A cancelled turn SHALL NOT emit a
completion telemetry event.

#### Scenario: Completed turn is measured
- **WHEN** a chat turn completes normally
- **THEN** a telemetry event is emitted recording the backend, the elapsed milliseconds, and any token counts the backend reported

#### Scenario: Token counts are optional
- **WHEN** a chat turn completes on a backend that does not report token counts
- **THEN** the telemetry event is still emitted with the backend and elapsed time, and the token counts are absent rather than fabricated

#### Scenario: Cancelled turn emits no completion event
- **WHEN** a chat turn is cancelled before completion
- **THEN** no chat-turn completion telemetry event is emitted

### Requirement: Logs never carry PII or transcript content

No log event, on any sink, SHALL contain Membership Household identifying data — names,
email addresses, postal addresses, or household identifiers — or chat prompt or reply
text. This SHALL be verified by an automated test over the chat-turn telemetry event.

#### Scenario: Chat telemetry event carries no message content
- **WHEN** a chat-turn telemetry event is emitted for a turn whose prompt and reply contain text
- **THEN** the event contains neither the prompt text nor the reply text

#### Scenario: Telemetry event carries no household identity
- **WHEN** a chat-turn telemetry event is emitted
- **THEN** the event contains no household name, email, address, or household identifier
