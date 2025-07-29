from dishka import make_async_container
from dishka.integrations.aiogram import AiogramProvider
from lib.observability import setup_observability

from .bot import bot_provider
from .clients import backend_client_provider
from .config import AppConfig

config = AppConfig()  # type: ignore

setup_observability(config.observability, service_name="telegram-client")


dishka_container = make_async_container(
    AiogramProvider(),
    config.dishka_provider(),
    bot_provider,
    backend_client_provider,
)
