from dishka import make_async_container
from dishka.integrations.fastapi import FastapiProvider, setup_dishka
from fastapi import FastAPI
from lib.fastapi import dishka_lifespan, setup_app
from lib.observability import setup_observability

from .agent import tags_agent_provider
from .config import AppConfig
from .router import router

config = AppConfig()  # type: ignore

setup_observability(config.observability, service_name="agent")


dishka_container = make_async_container(
    FastapiProvider(), config.dishka_provider(), tags_agent_provider
)


app = FastAPI(lifespan=dishka_lifespan, title="MemeXpert Agent")
setup_dishka(dishka_container, app)
setup_app(app)

app.include_router(router)
