from dataclasses import dataclass

from dishka import Provider, Scope
from pydantic import BaseModel, Field
from pydantic_ai import Agent, RunContext

from .config import LlmConfig

__all__ = [
    "tags_agent_provider",
    "TagsAgentContext",
    "TagsAgentAnswer",
    "get_tags_agent",
]

tags_agent_provider = Provider(scope=Scope.APP)


SYSTEM_PROMPT = """You are professional internet culture expert.
Generate list of 10-20 tags for the user-provided image. Tags must be in Russian, lowercase, short and concise.
Use existing tags if they are relevant, but feel free to create new ones.
"""


@dataclass
class TagsAgentContext:
    existing_tags: list[str]


class TagsAgentAnswer(BaseModel):
    tags: list[str] = Field(description="List of tags for the image")


@tags_agent_provider.provide
def get_tags_agent(
    llm_config: LlmConfig,
) -> Agent[TagsAgentContext, TagsAgentAnswer]:
    agent = Agent(
        model=llm_config.model,
        deps_type=TagsAgentContext,
        output_type=TagsAgentAnswer,
        system_prompt=SYSTEM_PROMPT,
    )

    @agent.system_prompt
    async def add_existing_tags(ctx: RunContext[TagsAgentContext]) -> str:
        if ctx.deps.existing_tags:
            return "Existing tags:\n" + "\n".join(
                f"- {tag}" for tag in ctx.deps.existing_tags
            )
        else:
            return "No existing tags found."

    return agent
