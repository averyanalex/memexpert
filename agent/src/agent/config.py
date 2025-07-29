from lib.config import BaseAppConfig
from pydantic import BaseModel


class LlmConfig(BaseModel):
    """LLM configuration."""

    model: str


class AppConfig(BaseAppConfig):
    llm: LlmConfig
