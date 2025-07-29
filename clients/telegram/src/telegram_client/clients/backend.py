import base64
from typing import AsyncGenerator, Self

from dishka import Provider, Scope
from httpx import Timeout
from meme_xpert_backend_client import Client
from meme_xpert_backend_client.api.default import (
    create_meme_memes_post,
    search_memes_memes_get,
)
from meme_xpert_backend_client.models.meme import Meme
from meme_xpert_backend_client.models.meme_create import MemeCreate
from opentelemetry.trace.propagation.tracecontext import TraceContextTextMapPropagator

from telegram_client.config import BackendApiConfig

backend_client_provider = Provider(scope=Scope.APP)


class BackendClientError(Exception):
    pass


class BackendClient:
    def __init__(self, config: BackendApiConfig) -> None:
        self._config = config
        self._client: Client | None = None

    async def __aenter__(self) -> Self:
        headers: dict[str, str] = {}
        TraceContextTextMapPropagator().inject(headers)

        self._client = Client(
            self._config.base_url,
            timeout=Timeout(self._config.timeout),
            headers=headers,
        )
        await self._client.__aenter__()
        return self

    async def __aexit__(self, exc_type, exc_val, exc_tb) -> None:
        if self._client is not None:
            await self._client.__aexit__(exc_type, exc_val, exc_tb)
            self._client = None

    async def create_meme(
        self, file_id: str, file_unique_id: str, file_data: bytes
    ) -> Meme:
        if self._client is None:
            raise RuntimeError("BackendClient must be used as async context manager")

        result = await create_meme_memes_post.asyncio(
            client=self._client,
            body=MemeCreate(
                file_id=file_id,
                file_unique_id=file_unique_id,
                file_data=base64.b64encode(file_data).decode("ascii"),
            ),
        )

        if not isinstance(result, Meme):
            raise BackendClientError(f"Failed to create meme: {result}")

        return result

    async def search_memes(self, query: str, limit: int = 10) -> list[Meme]:
        if self._client is None:
            raise RuntimeError("BackendClient must be used as async context manager")

        result = await search_memes_memes_get.asyncio(
            client=self._client,
            text=query,
            limit=limit,
        )

        if not isinstance(result, list):
            raise BackendClientError(f"Failed to search memes: {result}")

        return result


@backend_client_provider.provide
async def create_backend_client(
    config: BackendApiConfig,
) -> AsyncGenerator[BackendClient, BaseException]:
    async with BackendClient(config) as client:
        yield client
