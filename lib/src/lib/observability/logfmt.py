import logging


class LogfmtFormatter(logging.Formatter):
    def format(self, record: logging.LogRecord) -> str:
        # ANSI color codes
        RESET = "\033[0m"
        COLORS = {
            "DEBUG": "\033[36m",  # Cyan
            "INFO": "\033[32m",  # Green
            "WARNING": "\033[33m",  # Yellow
            "ERROR": "\033[31m",  # Red
            "CRITICAL": "\033[41m\033[97m",  # White on Red background
        }

        level_color = COLORS.get(record.levelname, "")
        time_color = "\033[90m"  # Bright black (gray)
        logger_color = "\033[35m"  # Magenta
        caller_color = "\033[94m"  # Bright blue
        trace_color = "\033[95m"  # Bright magenta
        msg_color = "\033[0m"  # Default

        logfmt = [
            f'{time_color}time="{self.formatTime(record, self.datefmt)}"{RESET}',
            f"{level_color}level={record.levelname}{RESET}",
            f'{logger_color}logger="{record.name}"{RESET}',
            f'{caller_color}caller="{record.pathname}:{record.lineno}"{RESET}',
        ]

        trace_id = record.__dict__.get("otelTraceID")
        if trace_id:
            logfmt.append(f"{trace_color}trace_id={trace_id}{RESET}")

        span_id = record.__dict__.get("otelSpanID")
        if span_id:
            logfmt.append(f"{trace_color}span_id={span_id}{RESET}")

        msg = record.getMessage().replace("\n", "\\n").replace('"', '\\"')
        logfmt.append(f'{msg_color}msg="{msg}"{RESET}')

        # Add exception information if available
        if record.exc_info:
            exc_color = "\033[91m"  # Bright red for exceptions
            exc_text = self.formatException(record.exc_info)
            # Format exception text for logfmt (escape quotes and newlines)
            exc_formatted = (
                exc_text  # exc_text.replace("\n", "\\n").replace('"', '\\"')
            )
            logfmt.append(f'{exc_color}exception="{exc_formatted}"{RESET}')

        return " ".join(logfmt)
