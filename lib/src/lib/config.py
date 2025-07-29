from dishka import Provider, Scope
from pydantic import BaseModel
from pydantic_settings import BaseSettings, SettingsConfigDict

from .observability.config import ObservabilityConfig


class BaseAppConfig(BaseSettings):
    observability: ObservabilityConfig = ObservabilityConfig()

    model_config = SettingsConfigDict(
        env_prefix="app__",
        env_file=".env",
        env_file_encoding="utf-8",
        env_nested_delimiter="__",
    )

    def dishka_provider(self) -> Provider:
        provider = Provider(scope=Scope.APP)

        def make_getter(v):
            def get_value():
                return v

            get_value.__annotations__["return"] = type(v)

            return get_value

        provider.provide(make_getter(self))

        for value in self.__dict__.values():
            if isinstance(value, BaseModel):
                provider.provide(make_getter(value))

        return provider
