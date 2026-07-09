from justapi import JustAPIApp
from pydantic import BaseModel

app = JustAPIApp()

class Item(BaseModel):
    name: str
    description: str | None = None
    price: float
    tax: float | None = None

@app.post("/items/")
async def create_item(item: Item):
    """
    Expects a JSON body matching the Item schema.
    Returns the created item with calculated total price.
    """
    item_dict = item.model_dump()
    if item.tax:
        price_with_tax = item.price + item.tax
        item_dict.update({"price_with_tax": price_with_tax})
    return item_dict

if __name__ == "__main__":
    app.run("127.0.0.1:8000")
