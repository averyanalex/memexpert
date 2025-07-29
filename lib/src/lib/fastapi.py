import logging
from contextlib import asynccontextmanager
from typing import AsyncGenerator

from fastapi import FastAPI, Request
from fastapi.encoders import jsonable_encoder
from fastapi.responses import JSONResponse
from opentelemetry import trace
from opentelemetry.instrumentation.fastapi import FastAPIInstrumentor

__all__ = ["setup_app", "dishka_lifespan"]


def configure_uvicorn_logging():
    """Configure uvicorn loggers to propagate to root logger."""

    uvicorn_loggers = ["uvicorn", "uvicorn.error", "uvicorn.access"]

    for logger_name in uvicorn_loggers:
        logger = logging.getLogger(logger_name)

        for handler in logger.handlers[:]:
            logger.removeHandler(handler)

        logger.propagate = True


async def internal_exception_handler(request: Request, exc: Exception):
    span = trace.get_current_span()
    span.record_exception(exc)
    span.set_status(trace.Status(trace.StatusCode.ERROR, str(exc)))

    return JSONResponse(
        status_code=500,
        content=jsonable_encoder({"code": 500, "msg": str(exc)}),
    )


@asynccontextmanager
async def dishka_lifespan(app: FastAPI) -> AsyncGenerator[None, None]:
    """Application lifespan handler for startup and shutdown events."""
    try:
        yield
    finally:
        await app.state.dishka_container.close()


def setup_app(app: FastAPI):
    configure_uvicorn_logging()

    app.add_exception_handler(Exception, internal_exception_handler)

    FastAPIInstrumentor.instrument_app(app)
