from aiogram import Router
from aiogram.types import InlineQuery, InlineQueryResultCachedPhoto
from dishka.integrations.aiogram import FromDishka

from telegram_client.clients import BackendClient

search_router = Router()


@search_router.inline_query()
async def search_handler(
    query: InlineQuery, backend_client: FromDishka[BackendClient]
) -> None:
    memes = await backend_client.search_memes(query.query, limit=50)

    results = []
    for meme in memes:
        results.append(
            InlineQueryResultCachedPhoto(
                id=str(meme.id),
                photo_file_id=meme.file_id,
            )
        )

    await query.answer(results)
