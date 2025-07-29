from lib.config import BaseAppConfig
from pydantic import BaseModel


class TelegramConfig(BaseModel):
    bot_token: str


class BackendApiConfig(BaseModel):
    base_url: str
    timeout: int = 60


class AppConfig(BaseAppConfig):
    backend_api: BackendApiConfig
    telegram: TelegramConfig
