from dishka.integrations.fastapi import FromDishka, inject
from fastapi import APIRouter

from .schemas import Meme, MemeCreate
from .services import MemeService

router = APIRouter()


@router.post("/memes", response_model=Meme)
@inject
async def create_meme(meme: MemeCreate, meme_service: FromDishka[MemeService]) -> Meme:
    return await meme_service.create_meme(meme)


@router.get("/memes", response_model=list[Meme])
@inject
async def search_memes(
    meme_service: FromDishka[MemeService], text: str | None = None, limit: int = 10
) -> list[Meme]:
    return await meme_service.search(text=text, limit=limit)
