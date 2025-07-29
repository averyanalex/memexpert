import importlib.util
import logging
import uuid

from opentelemetry import _logs as logs
from opentelemetry import metrics, trace
from opentelemetry.instrumentation.logging import LoggingInstrumentor
from opentelemetry.sdk._logs import LoggerProvider, LoggingHandler
from opentelemetry.sdk._logs.export import BatchLogRecordProcessor
from opentelemetry.sdk.metrics import MeterProvider
from opentelemetry.sdk.metrics.export import (
    PeriodicExportingMetricReader,
)
from opentelemetry.sdk.resources import (
    SERVICE_INSTANCE_ID,
    SERVICE_NAME,
    Resource,
)
from opentelemetry.sdk.trace import TracerProvider
from opentelemetry.sdk.trace.export import BatchSpanProcessor
from sentry_sdk.integrations.opentelemetry import SentrySpanProcessor

from .config import ObservabilityConfig
from .logfmt import LogfmtFormatter

logger = logging.getLogger(__name__)


def setup_httpx(config: ObservabilityConfig):
    if importlib.util.find_spec("httpx") is not None:
        try:
            from opentelemetry.instrumentation.httpx import HTTPXClientInstrumentor

            HTTPXClientInstrumentor().instrument()

            logger.info("httpx has been instrumented")
        except ImportError:
            logger.warning(
                "httpx instrumentation is not available, skipping httpx instrumentation"
            )

        if config.suppress_httpx_logs:
            logging.getLogger("httpx").setLevel(logging.WARNING)
            logging.getLogger("httpcore").setLevel(logging.WARNING)
            logger.info("httpx logs have been suppressed")


def setup_logging():
    LoggingInstrumentor().instrument()

    root_logger = logging.getLogger()
    root_logger.setLevel(logging.INFO)
    for handler in root_logger.handlers[:]:
        root_logger.removeHandler(handler)
    console_handler = logging.StreamHandler()
    console_handler.setFormatter(LogfmtFormatter())
    root_logger.addHandler(console_handler)

    logger.info("Logging has been setup")


def setup_observability(
    config: ObservabilityConfig = ObservabilityConfig(), service_name: str | None = None
):
    setup_logging()

    # RESOURCE
    resource = Resource.create(
        attributes={
            SERVICE_INSTANCE_ID: str(uuid.uuid4()),
        }
        | ({SERVICE_NAME: service_name} if service_name else {}),
    )

    # LOGGING
    logger_provider = LoggerProvider(resource=resource)

    if config.enable_console_logs:
        from opentelemetry.sdk._logs.export import ConsoleLogExporter

        logger_provider.add_log_record_processor(
            BatchLogRecordProcessor(ConsoleLogExporter())
        )
        logger.info("Enabled console logs exporter")

    if config.enable_otel_logs:
        from opentelemetry.exporter.otlp.proto.grpc._log_exporter import OTLPLogExporter

        logger_provider.add_log_record_processor(
            BatchLogRecordProcessor(OTLPLogExporter())
        )
        logger.info("Enabled opentelemetry logs exporter")

    logs.set_logger_provider(logger_provider)

    otel_handler = LoggingHandler(logger_provider=logger_provider)
    logging.getLogger().addHandler(otel_handler)

    # TRACING
    tracer_provider = TracerProvider(resource=resource)
    tracer_provider.add_span_processor(SentrySpanProcessor())

    if config.enable_console_tracer:
        from opentelemetry.sdk.trace.export import ConsoleSpanExporter

        tracer_provider.add_span_processor(BatchSpanProcessor(ConsoleSpanExporter()))
        logger.info("Enabled console span exporter")

    if config.enable_otel_tracer:
        from opentelemetry.exporter.otlp.proto.grpc.trace_exporter import (
            OTLPSpanExporter,
        )

        tracer_provider.add_span_processor(BatchSpanProcessor(OTLPSpanExporter()))
        logger.info("Enabled opentelemetry span exporter")

    trace.set_tracer_provider(tracer_provider)

    # METRICS
    metric_readers = []

    if config.enable_console_metrics:
        from opentelemetry.sdk.metrics.export import ConsoleMetricExporter

        metric_readers.append(PeriodicExportingMetricReader(ConsoleMetricExporter()))
        logger.info("Enabled console metrics exporter")

    if config.enable_otel_metrics:
        from opentelemetry.exporter.otlp.proto.grpc.metric_exporter import (
            OTLPMetricExporter,
        )

        metric_readers.append(PeriodicExportingMetricReader(OTLPMetricExporter()))
        logger.info("Enabled opentelemetry metrics exporter")

    meter_provider = MeterProvider(resource=resource, metric_readers=metric_readers)
    metrics.set_meter_provider(meter_provider)

    setup_httpx(config)
