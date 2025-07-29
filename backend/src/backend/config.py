from lib.config import BaseAppConfig
from pydantic import BaseModel


class AgentApiConfig(BaseModel):
    base_url: str
    timeout: int = 60


class DatabaseConfig(BaseModel):
    url: str


class AppConfig(BaseAppConfig):
    db: DatabaseConfig
    agent_api: AgentApiConfig
