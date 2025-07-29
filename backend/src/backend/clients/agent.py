from typing import AsyncGenerator, Self

from dishka import Provider, Scope
from httpx import Timeout
from meme_xpert_agent_client import Client
from meme_xpert_agent_client.api.generate import (
    generate_tags_generate_tags_post,
)
from meme_xpert_agent_client.models.generate_tags_request import GenerateTagsRequest
from meme_xpert_agent_client.models.generate_tags_response import GenerateTagsResponse
from opentelemetry.trace.propagation.tracecontext import TraceContextTextMapPropagator

from backend.config import AgentApiConfig

agent_client_provider = Provider(scope=Scope.APP)


class AgentClientError(Exception):
    pass


class AgentClient:
    def __init__(self, config: AgentApiConfig) -> None:
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

    async def generate_tags(self, image_data: str) -> list[str]:
        """Generate tags for an image using the AI agent."""
        if self._client is None:
            raise RuntimeError("AgentClient must be used as async context manager")

        tags_result = await generate_tags_generate_tags_post.asyncio(
            client=self._client,
            body=GenerateTagsRequest(
                image=image_data,
            ),
        )

        if not isinstance(tags_result, GenerateTagsResponse):
            raise AgentClientError(f"Failed to generate tags: {tags_result}")

        return tags_result.tags


@agent_client_provider.provide
async def get_agent_client(
    config: AgentApiConfig,
) -> AsyncGenerator[AgentClient, BaseException]:
    async with AgentClient(config) as client:
        yield client
