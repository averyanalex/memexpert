ARG PYTHON_VERSION="3.13"
ARG DEBIAN_VERSION="bookworm"


FROM docker.io/library/python:${PYTHON_VERSION}-slim-${DEBIAN_VERSION}

ENV UV_COMPILE_BYTECODE=1 UV_LINK_MODE=copy UV_PYTHON_DOWNLOADS=never VIRTUAL_ENV=/venv PATH="/venv/bin:$PATH"
ENV UVICORN_HOST="0.0.0.0" UVICORN_PORT=8000

RUN groupadd -g 999 -r app && \
    useradd -r -u 999 -g app -s /sbin/nologin app

WORKDIR /app/backend

COPY backend/uv.lock backend/pyproject.toml /app/backend/

RUN --mount=from=ghcr.io/astral-sh/uv,source=/uv,target=/usr/local/bin/uv \
    --mount=type=cache,target=/root/.cache/uv \
    uv sync --frozen --no-editable --active --no-dev --no-install-project --no-group local

COPY lib/ /app/lib/
COPY agent-client/ /app/agent-client/

RUN --mount=from=ghcr.io/astral-sh/uv,source=/uv,target=/usr/local/bin/uv \
    --mount=type=cache,target=/root/.cache/uv \
    uv sync --frozen --no-editable --active --no-dev --no-install-project

COPY backend/ /app/backend/

RUN --mount=from=ghcr.io/astral-sh/uv,source=/uv,target=/usr/local/bin/uv \
    --mount=type=cache,target=/root/.cache/uv \
    uv sync --frozen --no-editable --active --no-dev

USER app

CMD ["sh", "-c", "alembic upgrade head && uvicorn backend:app"]
