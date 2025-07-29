from dishka import Provider, Scope
from sqlalchemy.ext.asyncio import AsyncEngine
from sqlmodel.ext.asyncio.session import AsyncSession

from backend.clients.agent import AgentClient
from backend.models import Meme as MemeModel
from backend.repositories import memes, tags
from backend.schemas.meme import Meme, MemeCreate


class MemeService:
    def __init__(self, engine: AsyncEngine, agent_client: AgentClient) -> None:
        self._engine = engine
        self._agent_client = agent_client

    def _to_schema(self, meme_model: MemeModel) -> Meme:
        return Meme(
            id=meme_model.id,
            file_unique_id=meme_model.file_unique_id,
            file_id=meme_model.file_id,
            tags=[tag.name for tag in meme_model.tags],
        )

    async def search(self, text: str | None = None, limit: int = 10) -> list[Meme]:
        async with AsyncSession(self._engine) as session:
            if text:
                meme_models = await memes.find_by_tag_ilike(session, text, limit=limit)
            else:
                meme_models = await memes.find_all(session, limit=limit)

            return [self._to_schema(meme) for meme in meme_models]

    async def create_meme(self, meme_create: MemeCreate) -> Meme:
        generated_tags = await self._agent_client.generate_tags(meme_create.file_data)

        async with AsyncSession(self._engine) as session:
            # TODO: do just one db roundtrip

            tag_objects = await tags.upsert_tags(session, generated_tags)

            meme_model = memes.create(
                session,
                file_unique_id=meme_create.file_unique_id,
                file_id=meme_create.file_id,
                tags=tag_objects,
            )

            schema = self._to_schema(meme_model)

            await session.commit()

            return schema


meme_service_provider = Provider(scope=Scope.APP)
meme_service_provider.provide(MemeService)
