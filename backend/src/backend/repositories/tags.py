from sqlalchemy.dialects.postgresql import insert
from sqlmodel import col, select
from sqlmodel.ext.asyncio.session import AsyncSession

from backend.models import Tag


async def find_all(session: AsyncSession) -> list[str]:
    stmt = select(col(Tag.name))
    result = await session.exec(stmt)
    return list(result.all())


async def upsert_tags(session: AsyncSession, names: list[str]) -> list[Tag]:
    if not names:
        return []

    values_to_insert = [{"name": name} for name in names]

    insert_stmt = insert(Tag).values(values_to_insert)
    upsert_stmt = insert_stmt.on_conflict_do_update(
        index_elements=[Tag.name],
        set_={"updated_at": insert_stmt.excluded.updated_at},
    ).returning(Tag)

    result = await session.scalars(upsert_stmt)
    return list(result.all())
