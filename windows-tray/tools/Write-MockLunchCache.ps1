param(
  [string]$Date = (Get-Date -Format 'yyyy-MM-dd'),
  [string[]]$Languages = @('en', 'fi'),
  [switch]$ClearExisting,
  [switch]$DisableMockMode
)

$ErrorActionPreference = 'Stop'

$cacheDir = Join-Path $env:LOCALAPPDATA 'compass-lunch\cache'
New-Item -ItemType Directory -Force -Path $cacheDir | Out-Null
$mockModePath = Join-Path $cacheDir 'mock-cache-mode'

if ($DisableMockMode) {
  Remove-Item -Force -ErrorAction SilentlyContinue $mockModePath
  Write-Host "Removed $mockModePath"
  Write-Host "Restart compass-lunch.exe or refresh manually to return to live data."
  exit 0
}

if ($ClearExisting) {
  Remove-Item -Force -ErrorAction SilentlyContinue (Join-Path $cacheDir 'lunch-api__*.json')
  Remove-Item -Force -ErrorAction SilentlyContinue (Join-Path $cacheDir 'lunchapi__*.json')
}

Set-Content -Path $mockModePath -Value "Local mock cache mode enabled by Write-MockLunchCache.ps1" -Encoding ASCII

$restaurants = @(
  @{ id = 'snellmania'; order = 1; fi = 'Snellmania'; en = 'Snellmania'; url = 'https://www.compass-group.fi/ravintolat-ja-ruokalistat/foodco/kaupungit/kuopio/ita-suomen-yliopistosnellmania/'; status = 'serving'; hours = '10:30-12:30' },
  @{ id = 'cafe-snellari'; order = 2; fi = 'Cafe Snellari'; en = 'Cafe Snellari'; url = 'https://www.compass-group.fi/ravintolat-ja-ruokalistat/foodco/kaupungit/kuopio/cafe-snellari/'; status = 'serving'; hours = '10:30-13:30' },
  @{ id = 'canthia'; order = 3; fi = 'Canthia'; en = 'Canthia'; url = 'https://www.compass-group.fi/ravintolat-ja-ruokalistat/foodco/kaupungit/kuopio/ita-suomen-yliopistocanthia/'; status = 'closed'; hours = ''; closureDays = 9 },
  @{ id = 'tietoteknia'; order = 4; fi = 'Tietoteknia'; en = 'Tietoteknia'; url = 'https://www.compass-group.fi/ravintolat-ja-ruokalistat/foodco/kaupungit/kuopio/tietoteknia/'; status = 'serving'; hours = '10:30-14:00' },
  @{ id = 'hyva-huomen-bioteknia'; order = 5; fi = 'Hyva Huomen Bioteknia'; en = 'Hyva Huomen Bioteknia'; url = 'https://hyvahuomen.fi/bioteknia/'; status = 'serving'; hours = '10:30-13:00' },
  @{ id = 'antell-round'; order = 6; fi = 'Antell Round'; en = 'Antell Round'; url = 'https://antell.fi/lounas/kuopio/round/'; status = 'serving'; hours = '10:30-13:30' },
  @{ id = 'antell-highway'; order = 7; fi = 'Antell Highway'; en = 'Antell Highway'; url = 'https://antell.fi/lounas/kuopio/highway/'; status = 'noMenu'; hours = '' },
  @{ id = 'mediteknia'; order = 8; fi = 'Mediteknia'; en = 'Mediteknia'; url = 'https://www.compass-group.fi/ravintolat-ja-ruokalistat/foodco/kaupungit/kuopio/ita-suomen-yliopisto-mediteknia/'; status = 'serving'; hours = '10:30-13:30' },
  @{ id = 'pranzeria-sorrento'; order = 9; fi = 'Pranzeria Sorrento'; en = 'Pranzeria Sorrento'; url = 'https://www.sorrento.fi/pranzeria/'; status = 'serving'; hours = '10:30-14:00' },
  @{ id = 'caari'; order = 10; fi = 'Caari'; en = 'Caari'; url = 'https://www.compass-group.fi/ravintolat-ja-ruokalistat/foodco/kaupungit/kuopio/caari/'; status = 'serving'; hours = '10:30-13:30' }
)

$mockDishes = @{
  'snellmania' = @{
    fi = @{ main = 'Sitruunainen kirjolohipasta'; veg = 'Mausteinen tofu-kasviswokki'; side = 'Paahdetut perunat'; salad1 = 'Caesarsalaatti'; salad2 = 'Tomaatti-kurkkusalaatti'; dessert = 'Mustikkapiirakka' }
    en = @{ main = 'Lemon rainbow trout pasta'; veg = 'Spicy tofu vegetable wok'; side = 'Roasted potatoes'; salad1 = 'Caesar salad'; salad2 = 'Tomato cucumber salad'; dessert = 'Blueberry pie' }
    ingredients = @{ main = 'rainbow trout, pasta (WHEAT), cream, lemon juice, dill, onion, vegetable stock, rapeseed oil, iodized salt, black pepper'; veg = 'tofu (SOY), broccoli, carrot, bell pepper, ginger, garlic, sesame oil, soy sauce, chili, coriander, water, rice vinegar, modified corn starch'; dessert = 'blueberry, oat, WHEAT flour, sugar, margarine, cardamom, vanilla sugar, iodized salt' }
  }
  'cafe-snellari' = @{
    fi = @{ main = 'Broileria tomaatti-basilikakastikkeessa'; veg = 'Linssi-kookoscurry'; side = 'Yrttiohratto'; salad1 = 'Kreikkalainen salaatti'; salad2 = 'Punakaali-porkkanasalaatti'; dessert = 'Omenapaistos' }
    en = @{ main = 'Chicken in tomato basil sauce'; veg = 'Lentil coconut curry'; side = 'Herb barley risotto'; salad1 = 'Greek salad'; salad2 = 'Red cabbage carrot salad'; dessert = 'Apple crumble' }
    ingredients = @{ main = 'chicken, tomato, basil, onion, garlic, cream, vegetable stock, rapeseed oil, iodized salt'; veg = 'red lentils, coconut milk, tomato, ginger, garlic, coriander, cumin, turmeric, chili, lime juice'; dessert = 'apple, oat flakes, WHEAT flour, sugar, cinnamon, margarine, vanilla' }
  }
  'tietoteknia' = @{
    fi = @{ main = 'Savupaprika-possupata'; veg = 'Mifu gochujang-kastikkeessa'; side = 'Perunamuusi'; salad1 = 'Vihersalaatti ja siemeniä'; salad2 = 'Kurpitsa-papusalaatti'; dessert = 'Pannacotta ja marjakastike' }
    en = @{ main = 'Smoked paprika pork stew'; veg = 'Mifu in gochujang sauce'; side = 'Mashed potatoes'; salad1 = 'Green salad with seeds'; salad2 = 'Pumpkin bean salad'; dessert = 'Panna cotta with berry sauce' }
    ingredients = @{ main = 'pork, smoked paprika, tomato paste, onion, garlic, carrot, beef stock, thyme, rapeseed oil, iodized salt'; veg = 'MIFU milk protein, gochujang paste (SOY, WHEAT), onion, bell pepper, ginger, garlic, sesame seed, water'; dessert = 'cream, milk, sugar, gelatin, vanilla, raspberry, blackcurrant, lemon juice' }
  }
  'hyva-huomen-bioteknia' = @{
    fi = @{ main = 'Naudanlihapullat ja pippurikastike'; veg = 'Paahdettu kukkakaali-tahinikulho'; side = 'Valkosipuliriisi'; salad1 = 'Avokadosalaatti'; salad2 = 'Punajuurihummus'; dessert = 'Suklaamousse' }
    en = @{ main = 'Beef meatballs with pepper sauce'; veg = 'Roasted cauliflower tahini bowl'; side = 'Garlic rice'; salad1 = 'Avocado salad'; salad2 = 'Beetroot hummus'; dessert = 'Chocolate mousse' }
    ingredients = @{ main = 'beef, breadcrumbs (WHEAT), egg, onion, black pepper, cream, beef stock, mustard, iodized salt'; veg = 'cauliflower, chickpea, tahini (SESAME), lemon, garlic, parsley, cumin, olive oil, salt'; dessert = 'dark chocolate, cream, sugar, cocoa powder, vanilla, salt' }
  }
  'antell-round' = @{
    fi = @{ main = 'Ylikypsa naudan brisket'; veg = 'Falafel ja minttujogurtti'; side = 'Bataattiranskalaiset'; salad1 = 'Tabbouleh'; salad2 = 'Paahdettu paprika'; dessert = 'Sitruunapiirakka' }
    en = @{ main = 'Slow cooked beef brisket'; veg = 'Falafel with mint yoghurt'; side = 'Sweet potato fries'; salad1 = 'Tabbouleh'; salad2 = 'Roasted pepper salad'; dessert = 'Lemon tart' }
    ingredients = @{ main = 'beef brisket, barbecue sauce, tomato, onion, garlic, molasses, vinegar, mustard, black pepper'; veg = 'chickpea, parsley, coriander, cumin, garlic, onion, yoghurt, mint, lemon juice'; dessert = 'lemon juice, egg, sugar, butter, WHEAT flour, cream, vanilla' }
  }
  'mediteknia' = @{
    fi = @{ main = 'Seesami-lohi ja limedippi'; veg = 'Munakoiso-tomaattivuoka'; side = 'Jasmiiniriisi'; salad1 = 'Nuudelisalaatti'; salad2 = 'Edamame-papusalaatti'; dessert = 'Mangorahka' }
    en = @{ main = 'Sesame salmon with lime dip'; veg = 'Aubergine tomato bake'; side = 'Jasmine rice'; salad1 = 'Noodle salad'; salad2 = 'Edamame bean salad'; dessert = 'Mango quark' }
    ingredients = @{ main = 'salmon, sesame seed, lime, yoghurt, dill, rapeseed oil, iodized salt, white pepper'; veg = 'aubergine, tomato, mozzarella, basil, garlic, onion, olive oil, black pepper'; dessert = 'quark, mango puree, cream, sugar, vanilla, lemon juice' }
  }
  'pranzeria-sorrento' = @{
    fi = @{ main = 'Pasta arrabbiata ja pecorino'; veg = 'Sienirisotto'; side = 'Focaccia'; salad1 = 'Rucola-parmesansalaatti'; salad2 = 'Marinoidut oliivit'; dessert = 'Tiramisu' }
    en = @{ main = 'Pasta arrabbiata with pecorino'; veg = 'Mushroom risotto'; side = 'Focaccia'; salad1 = 'Rocket parmesan salad'; salad2 = 'Marinated olives'; dessert = 'Tiramisu' }
    ingredients = @{ main = 'pasta (WHEAT), tomato, chili, garlic, pecorino cheese, olive oil, basil, black pepper'; veg = 'risotto rice, mushroom, white wine, parmesan, onion, garlic, vegetable stock, butter'; dessert = 'mascarpone, coffee, ladyfinger biscuit (WHEAT), cocoa, egg, sugar' }
  }
  'caari' = @{
    fi = @{ main = 'Kalkkunaleike ja rosmariinikastike'; veg = 'Hernepihvit ja kaurafraiche'; side = 'Uunijuurekset'; salad1 = 'Kaali-omenasalaatti'; salad2 = 'Linssisalaatti'; dessert = 'Mansikkakiisseli' }
    en = @{ main = 'Turkey cutlet with rosemary sauce'; veg = 'Pea patties with oat fraiche'; side = 'Oven roasted root vegetables'; salad1 = 'Cabbage apple salad'; salad2 = 'Lentil salad'; dessert = 'Strawberry soup' }
    ingredients = @{ main = 'turkey, rosemary, cream, chicken stock, onion, rapeseed oil, iodized salt, white pepper'; veg = 'pea, oat fraiche, potato starch, onion, parsley, rapeseed oil, lemon juice, salt'; dessert = 'strawberry, water, sugar, potato starch, vanilla' }
  }
}

function New-Price {
  param([string]$Amount, [string[]]$Audiences)
  [ordered]@{
    amount = $Amount
    currency = 'EUR'
    audiences = $Audiences
  }
}

function New-Recipe {
  param([string]$Id, [string]$Name, [string]$Ingredients, [string[]]$Diets)
  [ordered]@{
    id = $Id
    name = $Name
    ingredients = $Ingredients
    nutritionPer100g = @(
      [ordered]@{ name = 'EnergyKcal'; amount = 142; unit = 'kcal' },
      [ordered]@{ name = 'Protein'; amount = 7.4; unit = 'g' },
      [ordered]@{ name = 'Carbohydrates'; amount = 19.5; unit = 'g' },
      [ordered]@{ name = 'Fat'; amount = 3.6; unit = 'g' }
    )
    co2eKilogramsPer100Grams = 0.25
    diets = $Diets
  }
}

function New-LunchItem {
  param([string]$Id, [string]$Name, [string[]]$Tags, [object]$Recipe)
  $item = [ordered]@{
    id = $Id
    name = $Name
    tags = $Tags
  }
  if ($null -ne $Recipe) {
    $item.recipe = $Recipe
  }
  $item
}

function New-ServingMenu {
  param([hashtable]$Restaurant, [string]$Language)

  $dishSet = $mockDishes[$Restaurant.id]
  if ($null -eq $dishSet) {
    $dishSet = $mockDishes['snellmania']
  }
  $dishes = $dishSet[$Language]
  if ($null -eq $dishes) {
    $dishes = $dishSet['en']
  }
  $ingredients = $dishSet['ingredients']

  $mainTitle = if ($Language -eq 'fi') { 'Paaruoka' } else { 'Main course' }
  $vegTitle = if ($Language -eq 'fi') { 'Kasvislounas' } else { 'Vegetarian lunch' }
  $dessertTitle = if ($Language -eq 'fi') { 'Jalkiruoka' } else { 'Dessert' }
  $saladTitle = if ($Language -eq 'fi') { 'Salaattibuffet' } else { 'Salad buffet' }

  $riceName = if ($Language -eq 'fi') { 'Riisi' } else { 'Rice' }
  $sideName = $dishes.side
  $mainDishName = $dishes.main
  $vegDishName = $dishes.veg
  $saladOneName = $dishes.salad1
  $saladTwoName = $dishes.salad2
  $dessertName = $dishes.dessert

  $mainRecipe = New-Recipe -Id "$($Restaurant.id)-recipe-main-1" -Name $mainDishName -Ingredients $ingredients.main -Diets @('A', 'L')
  $mainRiceRecipe = New-Recipe -Id "$($Restaurant.id)-recipe-rice-main" -Name $riceName -Ingredients 'rice, water, iodized salt, rapeseed oil' -Diets @('G', 'L', 'M', 'Veg')
  $vegRecipe = New-Recipe -Id "$($Restaurant.id)-recipe-veg-1" -Name $vegDishName -Ingredients $ingredients.veg -Diets @('G', 'L', 'M', 'Veg', 'VS')
  $vegRiceRecipe = New-Recipe -Id "$($Restaurant.id)-recipe-rice-veg" -Name $riceName -Ingredients 'rice, water, iodized salt' -Diets @('G', 'L', 'M', 'Veg')
  $dessertRecipe = New-Recipe -Id "$($Restaurant.id)-recipe-dessert-1" -Name $dessertName -Ingredients $ingredients.dessert -Diets @('A', 'L', 'M')

  @(
    [ordered]@{
      id = "$($Restaurant.id)-main"
      title = $mainTitle
      prices = @(
        (New-Price -Amount '3.10' -Audiences @('student'))
        (New-Price -Amount '6.20' -Audiences @('staff'))
        (New-Price -Amount '12.90' -Audiences @('guest'))
      )
      items = @(
        (New-LunchItem -Id "$($Restaurant.id)-main-1" -Name $mainDishName -Tags @('A', 'L') -Recipe $mainRecipe)
        (New-LunchItem -Id "$($Restaurant.id)-main-2" -Name $riceName -Tags @('G', 'L', 'M', 'Veg') -Recipe $mainRiceRecipe)
        (New-LunchItem -Id "$($Restaurant.id)-main-3" -Name $sideName -Tags @('G', 'L', 'M', 'Veg') -Recipe $null)
      )
      sortOrder = 1
    },
    [ordered]@{
      id = "$($Restaurant.id)-veg"
      title = $vegTitle
      prices = @(
        (New-Price -Amount '2.95' -Audiences @('student'))
        (New-Price -Amount '5.40' -Audiences @('staff'))
        (New-Price -Amount '11.50' -Audiences @('guest'))
      )
      items = @(
        (New-LunchItem -Id "$($Restaurant.id)-veg-1" -Name $vegDishName -Tags @('*', 'G', 'L', 'M', 'Veg', 'VS') -Recipe $vegRecipe)
        (New-LunchItem -Id "$($Restaurant.id)-veg-2" -Name $riceName -Tags @('G', 'L', 'M', 'Veg') -Recipe $vegRiceRecipe)
      )
      sortOrder = 2
    },
    [ordered]@{
      id = "$($Restaurant.id)-salad"
      title = $saladTitle
      prices = @(
        (New-Price -Amount '4.90' -Audiences @('student', 'staff', 'guest'))
      )
      items = @(
        (New-LunchItem -Id "$($Restaurant.id)-salad-1" -Name $saladOneName -Tags @('A', 'L') -Recipe $null)
        (New-LunchItem -Id "$($Restaurant.id)-salad-2" -Name $saladTwoName -Tags @('G', 'L', 'M', 'Veg') -Recipe $null)
      )
      sortOrder = 3
    },
    [ordered]@{
      id = "$($Restaurant.id)-dessert"
      title = $dessertTitle
      prices = @(
        (New-Price -Amount '1.80' -Audiences @('student', 'staff', 'guest'))
      )
      items = @(
        (New-LunchItem -Id "$($Restaurant.id)-dessert-1" -Name $dessertName -Tags @('A', 'L', 'M') -Recipe $dessertRecipe)
      )
      sortOrder = 4
    }
  )
}

function New-MenuPayload {
  param([hashtable]$Restaurant, [string]$Language, [string]$Date)

  $payload = [ordered]@{
    apiVersion = 'v1'
    schemaVersion = 1
    restaurant = [ordered]@{
      id = $Restaurant.id
      order = $Restaurant.order
      name = [ordered]@{ fi = $Restaurant.fi; en = $Restaurant.en }
      websiteUrl = $Restaurant.url
      languages = @('fi', 'en')
      closures = @()
    }
    requestedLanguage = $Language
    contentLanguage = $Language
    date = $Date
    service = [ordered]@{
      status = $Restaurant.status
      hours = $Restaurant.hours
    }
    offers = @()
    groups = @()
    freshness = [ordered]@{
      fetchedAt = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ss.fffZ')
      isStale = $false
    }
  }

  if ($Restaurant.status -eq 'closed') {
    $endsOn = (Get-Date $Date).AddDays([int]$Restaurant.closureDays).ToString('yyyy-MM-dd')
    $payload['closure'] = [ordered]@{
      kind = 'exceptional'
      startsOn = $Date
      endsOn = $endsOn
      reason = $(if ($Language -eq 'fi') { 'Mock-sulku testausta varten' } else { 'Mock closure for testing' })
    }
  } else {
    $payload['offers'] = @(
      [ordered]@{
        id = "$($Restaurant.id)-offer"
        label = $(if ($Language -eq 'fi') { 'Lounas' } else { 'Lunch' })
        price = (New-Price -Amount '11.00' -Audiences @('guest'))
        description = $(if ($Language -eq 'fi') { 'Mock-tarjous' } else { 'Mock offer' })
      }
    )
    $payload['groups'] = (New-ServingMenu -Restaurant $Restaurant -Language $Language)
  }

  $payload
}

foreach ($language in $Languages) {
  foreach ($restaurant in $restaurants) {
    $payload = New-MenuPayload $restaurant $language $Date
    $json = $payload | ConvertTo-Json -Depth 32
    $path = Join-Path $cacheDir "lunch-api__$($restaurant.id)__$language.json"
    $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText($path, $json, $utf8NoBom)
    Write-Host "Wrote $path"
  }
}

Write-Host ""
Write-Host "Mock cache date: $Date"
Write-Host "Mock cache mode: enabled ($mockModePath)"
Write-Host "Restart compass-lunch.exe to load these files from disk."
Write-Host "Run this script with -DisableMockMode to re-enable live API refreshes."
