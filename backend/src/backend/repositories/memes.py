from sqlalchemy.orm import selectinload
from sqlmodel import col, select
from sqlmodel.ext.asyncio.session import AsyncSession

from backend.models import Meme, MemeTagLink, Tag


async def find_all(session: AsyncSession, limit: int = 10) -> list[Meme]:
    stmt = (
        select(Meme)
        .options(selectinload(Meme.tags))  # type:ignore
        .limit(limit)
    )
    result = await session.exec(stmt)
    return list(result.all())


async def find_by_tag_ilike(
    session: AsyncSession, text: str, limit: int = 10
) -> list[Meme]:
    stmt = (
        select(Meme)
        .options(selectinload(Meme.tags))  # type:ignore
        .join(MemeTagLink, col(Meme.id) == col(MemeTagLink.meme_id))
        .join(Tag, col(MemeTagLink.tag_id) == col(Tag.id))
        .where(col(Tag.name).ilike(f"%{text}%"))
        .distinct()
        .limit(limit)
    )
    result = await session.exec(stmt)
    return list(result.all())


def create(
    session: AsyncSession, file_unique_id: str, file_id: str, tags: list[Tag]
) -> Meme:
    meme = Meme(
        file_unique_id=file_unique_id,
        file_id=file_id,
        tags=tags,
    )
    session.add(meme)
    return meme
