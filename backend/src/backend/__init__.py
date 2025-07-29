from dishka import make_async_container
from dishka.integrations.fastapi import FastapiProvider, setup_dishka
from fastapi import FastAPI
from lib.fastapi import dishka_lifespan, setup_app
from lib.observability import setup_observability

from .clients import agent_client_provider
from .config import AppConfig
from .db import db_provider
from .router import router
from .services import meme_service_provider

config = AppConfig()  # type: ignore

setup_observability(config.observability, service_name="backend")


dishka_container = make_async_container(
    FastapiProvider(),
    config.dishka_provider(),
    db_provider,
    agent_client_provider,
    meme_service_provider,
)


app = FastAPI(lifespan=dishka_lifespan, title="MemeXpert backend")
setup_dishka(dishka_container, app)
setup_app(app)

app.include_router(router)
