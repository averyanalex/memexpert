from dishka import Provider, Scope
from sqlalchemy.ext.asyncio import AsyncEngine, create_async_engine

from .config import DatabaseConfig


def db_engine(settings: DatabaseConfig) -> AsyncEngine:
    return create_async_engine(settings.url)


db_provider = Provider()
db_provider.provide(db_engine, scope=Scope.APP)
