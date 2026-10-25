#!/usr/bin/env python3
"""
Compiles curated global birding hotspots (eBird + IBA + national parks/reserves)
and major global cities into data/gazetteer.json, and maps regional codes
to confirmed species taxon IDs from models/taxonomy.json into data/regional_checklists.json.
"""

import json
import os
import sys
from typing import Dict, List, Set, Any

# Root directory of avian_rearranger
REPO_ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
TAXONOMY_PATH = os.path.join(REPO_ROOT, "models", "taxonomy.json")
DATA_DIR = os.path.join(REPO_ROOT, "data")
GAZETTEER_PATH = os.path.join(DATA_DIR, "gazetteer.json")
CHECKLISTS_PATH = os.path.join(DATA_DIR, "regional_checklists.json")


# -------------------------------------------------------------------------
# Curated Global Gazetteer: Hotspots, Reserves, and Cities
# -------------------------------------------------------------------------
RAW_GAZETTEER: List[Dict[str, Any]] = [
    # --- India: Karnataka (IN-KA) ---
    {"name": "Saul Kere", "region_code": "IN-KA", "lat": 12.9123, "lon": 77.6845, "category": "hotspot"},
    {"name": "Saul Kere Lake", "region_code": "IN-KA", "lat": 12.9123, "lon": 77.6845, "category": "hotspot"},
    {"name": "Bengaluru", "region_code": "IN-KA", "lat": 12.9716, "lon": 77.5946, "category": "city"},
    {"name": "Bangalore", "region_code": "IN-KA", "lat": 12.9716, "lon": 77.5946, "category": "city"},
    {"name": "Kaikondrahalli Lake", "region_code": "IN-KA", "lat": 12.9135, "lon": 77.6744, "category": "hotspot"},
    {"name": "Hebbal Lake", "region_code": "IN-KA", "lat": 13.0456, "lon": 77.5901, "category": "hotspot"},
    {"name": "Bannerghatta National Park", "region_code": "IN-KA", "lat": 12.8006, "lon": 77.5772, "category": "reserve"},
    {"name": "Ranganathittu Bird Sanctuary", "region_code": "IN-KA", "lat": 12.4244, "lon": 76.6575, "category": "reserve"},
    {"name": "Mysore", "region_code": "IN-KA", "lat": 12.2958, "lon": 76.6394, "category": "city"},
    {"name": "Mysuru", "region_code": "IN-KA", "lat": 12.2958, "lon": 76.6394, "category": "city"},
    {"name": "Nagarhole National Park", "region_code": "IN-KA", "lat": 12.0311, "lon": 76.1557, "category": "reserve"},
    {"name": "Bandipur National Park", "region_code": "IN-KA", "lat": 11.6664, "lon": 76.6291, "category": "reserve"},
    {"name": "Dandeli Wildlife Sanctuary", "region_code": "IN-KA", "lat": 15.2427, "lon": 74.6289, "category": "reserve"},
    {"name": "Anshi National Park", "region_code": "IN-KA", "lat": 14.9981, "lon": 74.3586, "category": "reserve"},
    {"name": "K Gudi", "region_code": "IN-KA", "lat": 11.9961, "lon": 77.1350, "category": "hotspot"},
    {"name": "Kyathadevara Gudi", "region_code": "IN-KA", "lat": 11.9961, "lon": 77.1350, "category": "hotspot"},
    {"name": "BR Hills", "region_code": "IN-KA", "lat": 11.9961, "lon": 77.1350, "category": "reserve"},
    {"name": "Biligiriranga Hills", "region_code": "IN-KA", "lat": 11.9961, "lon": 77.1350, "category": "reserve"},
    {"name": "Coorg", "region_code": "IN-KA", "lat": 12.4244, "lon": 75.7382, "category": "hotspot"},
    {"name": "Kodagu", "region_code": "IN-KA", "lat": 12.4244, "lon": 75.7382, "category": "hotspot"},
    {"name": "Madikeri", "region_code": "IN-KA", "lat": 12.4244, "lon": 75.7382, "category": "city"},
    {"name": "Kanaka Sarovara", "region_code": "IN-KA", "lat": 12.9716, "lon": 77.5946, "category": "hotspot"},

    # --- India: Rajasthan (IN-RJ) ---
    {"name": "Ranthambore", "region_code": "IN-RJ", "lat": 26.0173, "lon": 76.5026, "category": "reserve"},
    {"name": "Ranthambore National Park", "region_code": "IN-RJ", "lat": 26.0173, "lon": 76.5026, "category": "reserve"},
    {"name": "Keoladeo National Park", "region_code": "IN-RJ", "lat": 27.1594, "lon": 77.5218, "category": "reserve"},
    {"name": "Bharatpur", "region_code": "IN-RJ", "lat": 27.2152, "lon": 77.5030, "category": "city"},
    {"name": "Jaipur", "region_code": "IN-RJ", "lat": 26.9124, "lon": 75.7873, "category": "city"},
    {"name": "Desert National Park", "region_code": "IN-RJ", "lat": 27.0397, "lon": 70.8872, "category": "reserve"},
    {"name": "Jaisalmer", "region_code": "IN-RJ", "lat": 26.9157, "lon": 70.9083, "category": "city"},
    {"name": "Jhalana", "region_code": "IN-RJ", "lat": 26.8580, "lon": 75.8230, "category": "reserve"},
    {"name": "Jhalana Safari Park", "region_code": "IN-RJ", "lat": 26.8580, "lon": 75.8230, "category": "reserve"},
    {"name": "Jhalana Leopard Safari Park", "region_code": "IN-RJ", "lat": 26.8580, "lon": 75.8230, "category": "reserve"},

    # --- India: Gujarat (IN-GJ) ---
    {"name": "Gir", "region_code": "IN-GJ", "lat": 21.1243, "lon": 70.8242, "category": "reserve"},
    {"name": "Gir National Park", "region_code": "IN-GJ", "lat": 21.1243, "lon": 70.8242, "category": "reserve"},
    {"name": "Sasan Gir", "region_code": "IN-GJ", "lat": 21.1243, "lon": 70.8242, "category": "reserve"},
    {"name": "Velavadar Blackbuck National Park", "region_code": "IN-GJ", "lat": 21.9333, "lon": 72.0333, "category": "reserve"},
    {"name": "Little Rann of Kutch", "region_code": "IN-GJ", "lat": 23.2500, "lon": 71.2500, "category": "reserve"},
    {"name": "Ahmedabad", "region_code": "IN-GJ", "lat": 23.0225, "lon": 72.5714, "category": "city"},

    # --- India: Ladakh (IN-LA) ---
    {"name": "Leh", "region_code": "IN-LA", "lat": 34.1526, "lon": 77.5771, "category": "city"},
    {"name": "Pangong Tso", "region_code": "IN-LA", "lat": 33.7595, "lon": 78.6674, "category": "hotspot"},
    {"name": "Tso Moriri", "region_code": "IN-LA", "lat": 32.9000, "lon": 78.3167, "category": "hotspot"},
    {"name": "Shey Marshes", "region_code": "IN-LA", "lat": 34.0722, "lon": 77.6339, "category": "hotspot"},
    {"name": "Hemis National Park", "region_code": "IN-LA", "lat": 33.9870, "lon": 77.3450, "category": "reserve"},

    # --- India: Uttarakhand (IN-UT) ---
    {"name": "Jim Corbett National Park", "region_code": "IN-UT", "lat": 29.5300, "lon": 78.7747, "category": "reserve"},
    {"name": "Corbett", "region_code": "IN-UT", "lat": 29.5300, "lon": 78.7747, "category": "reserve"},
    {"name": "Pangot", "region_code": "IN-UT", "lat": 29.4167, "lon": 79.4333, "category": "hotspot"},
    {"name": "Dehradun", "region_code": "IN-UT", "lat": 30.3165, "lon": 78.0322, "category": "city"},

    # --- India: Assam (IN-AS) ---
    {"name": "Kaziranga National Park", "region_code": "IN-AS", "lat": 26.5775, "lon": 93.1711, "category": "reserve"},
    {"name": "Manas National Park", "region_code": "IN-AS", "lat": 26.7167, "lon": 91.0167, "category": "reserve"},
    {"name": "Guwahati", "region_code": "IN-AS", "lat": 26.1445, "lon": 91.7362, "category": "city"},

    # --- India: West Bengal (IN-WB) ---
    {"name": "Sundarbans National Park", "region_code": "IN-WB", "lat": 21.9497, "lon": 89.1833, "category": "reserve"},
    {"name": "Kolkata", "region_code": "IN-WB", "lat": 22.5726, "lon": 88.3639, "category": "city"},

    # --- India: Kerala (IN-KL) ---
    {"name": "Thattekad Bird Sanctuary", "region_code": "IN-KL", "lat": 10.1333, "lon": 76.6833, "category": "reserve"},
    {"name": "Periyar National Park", "region_code": "IN-KL", "lat": 9.4667, "lon": 77.2333, "category": "reserve"},
    {"name": "Munnar", "region_code": "IN-KL", "lat": 10.0889, "lon": 77.0595, "category": "hotspot"},
    {"name": "Kochi", "region_code": "IN-KL", "lat": 9.9312, "lon": 76.2673, "category": "city"},

    # --- India: Tamil Nadu (IN-TN) ---
    {"name": "Vedanthangal Bird Sanctuary", "region_code": "IN-TN", "lat": 12.5444, "lon": 79.8550, "category": "reserve"},
    {"name": "Mudumalai National Park", "region_code": "IN-TN", "lat": 11.5622, "lon": 76.5342, "category": "reserve"},
    {"name": "Coonoor", "region_code": "IN-TN", "lat": 11.3530, "lon": 76.7959, "category": "hotspot"},
    {"name": "Nilgiris", "region_code": "IN-TN", "lat": 11.4102, "lon": 76.6950, "category": "hotspot"},
    {"name": "Ooty", "region_code": "IN-TN", "lat": 11.4102, "lon": 76.6950, "category": "city"},
    {"name": "Chennai", "region_code": "IN-TN", "lat": 13.0827, "lon": 80.2707, "category": "city"},

    # --- India: Maharashtra (IN-MH) ---
    {"name": "Tadoba Andhari Tiger Reserve", "region_code": "IN-MH", "lat": 20.2450, "lon": 79.3050, "category": "reserve"},
    {"name": "Sanjay Gandhi National Park", "region_code": "IN-MH", "lat": 19.2288, "lon": 72.9182, "category": "reserve"},
    {"name": "Mumbai", "region_code": "IN-MH", "lat": 19.0760, "lon": 72.8777, "category": "city"},

    # --- India: Delhi / NCR (IN-DL, IN-HR) ---
    {"name": "Delhi", "region_code": "IN-DL", "lat": 28.6139, "lon": 77.2090, "category": "city"},
    {"name": "New Delhi", "region_code": "IN-DL", "lat": 28.6139, "lon": 77.2090, "category": "city"},
    {"name": "Okhla Bird Sanctuary", "region_code": "IN-DL", "lat": 28.5630, "lon": 77.3060, "category": "hotspot"},
    {"name": "Sultanpur National Park", "region_code": "IN-HR", "lat": 28.4628, "lon": 76.8942, "category": "reserve"},

    # --- United States: Florida (US-FL) ---
    {"name": "Everglades", "region_code": "US-FL", "lat": 25.2866, "lon": -80.8987, "category": "reserve"},
    {"name": "Everglades National Park", "region_code": "US-FL", "lat": 25.2866, "lon": -80.8987, "category": "reserve"},
    {"name": "Corkscrew Swamp Sanctuary", "region_code": "US-FL", "lat": 26.3756, "lon": -81.6033, "category": "reserve"},
    {"name": "Dry Tortugas National Park", "region_code": "US-FL", "lat": 24.6285, "lon": -82.8732, "category": "reserve"},
    {"name": "Miami", "region_code": "US-FL", "lat": 25.7617, "lon": -80.1918, "category": "city"},
    {"name": "Orlando", "region_code": "US-FL", "lat": 28.5383, "lon": -81.3792, "category": "city"},

    # --- United States: New York (US-NY) ---
    {"name": "Central Park", "region_code": "US-NY", "lat": 40.7851, "lon": -73.9683, "category": "hotspot"},
    {"name": "Jamaica Bay Wildlife Refuge", "region_code": "US-NY", "lat": 40.6175, "lon": -73.8344, "category": "hotspot"},
    {"name": "New York", "region_code": "US-NY", "lat": 40.7128, "lon": -74.0060, "category": "city"},
    {"name": "New York City", "region_code": "US-NY", "lat": 40.7128, "lon": -74.0060, "category": "city"},

    # --- United States: California (US-CA) ---
    {"name": "Yosemite National Park", "region_code": "US-CA", "lat": 37.8651, "lon": -119.5383, "category": "reserve"},
    {"name": "Point Reyes National Seashore", "region_code": "US-CA", "lat": 38.0694, "lon": -122.8805, "category": "reserve"},
    {"name": "Monterey Bay", "region_code": "US-CA", "lat": 36.6002, "lon": -121.8947, "category": "hotspot"},
    {"name": "San Francisco", "region_code": "US-CA", "lat": 37.7749, "lon": -122.4194, "category": "city"},
    {"name": "Los Angeles", "region_code": "US-CA", "lat": 34.0522, "lon": -118.2437, "category": "city"},

    # --- United States: Texas (US-TX) ---
    {"name": "Santa Ana National Wildlife Refuge", "region_code": "US-TX", "lat": 26.0792, "lon": -98.1383, "category": "reserve"},
    {"name": "High Island", "region_code": "US-TX", "lat": 29.5667, "lon": -94.3833, "category": "hotspot"},
    {"name": "Austin", "region_code": "US-TX", "lat": 30.2672, "lon": -97.7431, "category": "city"},
    {"name": "Houston", "region_code": "US-TX", "lat": 29.7604, "lon": -95.3698, "category": "city"},

    # --- United States: Arizona (US-AZ) ---
    {"name": "Ramsey Canyon Preserve", "region_code": "US-AZ", "lat": 31.4483, "lon": -110.3117, "category": "hotspot"},
    {"name": "Tucson", "region_code": "US-AZ", "lat": 32.2226, "lon": -110.9747, "category": "city"},

    # --- United States: Alaska (US-AK) ---
    {"name": "Denali National Park", "region_code": "US-AK", "lat": 63.1148, "lon": -151.1926, "category": "reserve"},
    {"name": "Anchorage", "region_code": "US-AK", "lat": 61.2181, "lon": -149.9003, "category": "city"},

    # --- United States: Washington (US-WA) ---
    {"name": "Olympic National Park", "region_code": "US-WA", "lat": 47.8021, "lon": -123.6044, "category": "reserve"},
    {"name": "Seattle", "region_code": "US-WA", "lat": 47.6062, "lon": -122.3321, "category": "city"},

    # --- United States: Colorado (US-CO) ---
    {"name": "Rocky Mountain National Park", "region_code": "US-CO", "lat": 40.3428, "lon": -105.6836, "category": "reserve"},
    {"name": "Denver", "region_code": "US-CO", "lat": 39.7392, "lon": -104.9903, "category": "city"},

    # --- South Africa (ZA) ---
    {"name": "Kruger", "region_code": "ZA", "lat": -23.9884, "lon": 31.5547, "category": "reserve"},
    {"name": "Kruger National Park", "region_code": "ZA", "lat": -23.9884, "lon": 31.5547, "category": "reserve"},
    {"name": "Cape Town", "region_code": "ZA", "lat": -33.9249, "lon": 18.4241, "category": "city"},
    {"name": "Johannesburg", "region_code": "ZA", "lat": -26.2041, "lon": 28.0473, "category": "city"},
    {"name": "Kirstenbosch National Botanical Garden", "region_code": "ZA", "lat": -33.9881, "lon": 18.4323, "category": "hotspot"},
    {"name": "Kgalagadi Transfrontier Park", "region_code": "ZA", "lat": -25.7667, "lon": 20.3833, "category": "reserve"},

    # --- Kenya (KE / KE-01) ---
    {"name": "Maasai Mara National Reserve", "region_code": "KE", "lat": -1.4933, "lon": 35.1444, "category": "reserve"},
    {"name": "Lake Nakuru National Park", "region_code": "KE", "lat": -0.3667, "lon": 36.0833, "category": "reserve"},
    {"name": "Nairobi", "region_code": "KE", "lat": -1.2921, "lon": 36.8219, "category": "city"},
    {"name": "Nairobi National Park", "region_code": "KE", "lat": -1.3733, "lon": 36.8589, "category": "reserve"},

    # --- Tanzania (TZ) ---
    {"name": "Serengeti National Park", "region_code": "TZ", "lat": -2.3333, "lon": 34.8333, "category": "reserve"},
    {"name": "Ngorongoro Crater", "region_code": "TZ", "lat": -3.1667, "lon": 35.5833, "category": "reserve"},

    # --- Botswana (BW) ---
    {"name": "Okavango Delta", "region_code": "BW", "lat": -19.2833, "lon": 22.9000, "category": "reserve"},
    {"name": "Chobe National Park", "region_code": "BW", "lat": -18.6667, "lon": 24.5000, "category": "reserve"},

    # --- Costa Rica (CR) ---
    {"name": "Monteverde Cloud Forest", "region_code": "CR", "lat": 10.3000, "lon": -84.8000, "category": "reserve"},
    {"name": "Manuel Antonio National Park", "region_code": "CR", "lat": 9.3853, "lon": -84.1436, "category": "reserve"},
    {"name": "San Jose", "region_code": "CR", "lat": 9.9281, "lon": -84.0907, "category": "city"},

    # --- Ecuador (EC) ---
    {"name": "Galapagos National Park", "region_code": "EC", "lat": -0.6667, "lon": -90.5500, "category": "reserve"},
    {"name": "Mindo Cloud Forest", "region_code": "EC", "lat": -0.0500, "lon": -78.7833, "category": "hotspot"},
    {"name": "Quito", "region_code": "EC", "lat": -0.1807, "lon": -78.4678, "category": "city"},

    # --- Peru (PE) ---
    {"name": "Manu National Park", "region_code": "PE", "lat": -12.2500, "lon": -71.5000, "category": "reserve"},
    {"name": "Lima", "region_code": "PE", "lat": -12.0464, "lon": -77.0428, "category": "city"},

    # --- Brazil (BR) ---
    {"name": "Pantanal", "region_code": "BR", "lat": -17.5000, "lon": -56.5000, "category": "reserve"},
    {"name": "Iguazu Falls", "region_code": "BR", "lat": -25.6953, "lon": -54.4367, "category": "reserve"},

    # --- United Kingdom (GB) ---
    {"name": "RSPB Minsmere", "region_code": "GB", "lat": 52.2472, "lon": 1.6153, "category": "reserve"},
    {"name": "London", "region_code": "GB", "lat": 51.5074, "lon": -0.1278, "category": "city"},

    # --- Spain (ES) ---
    {"name": "Donana National Park", "region_code": "ES", "lat": 36.9667, "lon": -6.4167, "category": "reserve"},
    {"name": "Madrid", "region_code": "ES", "lat": 40.4168, "lon": -3.7038, "category": "city"},

    # --- France (FR) ---
    {"name": "Camargue", "region_code": "FR", "lat": 43.5333, "lon": 4.6333, "category": "reserve"},
    {"name": "Paris", "region_code": "FR", "lat": 48.8566, "lon": 2.3522, "category": "city"},

    # --- Australia (AU, AU-NT, AU-QLD, AU-NSW, AU-VIC) ---
    {"name": "Kakadu National Park", "region_code": "AU-NT", "lat": -12.8333, "lon": 132.8333, "category": "reserve"},
    {"name": "Daintree Rainforest", "region_code": "AU-QLD", "lat": -16.2833, "lon": 145.4167, "category": "reserve"},
    {"name": "Sydney", "region_code": "AU-NSW", "lat": -33.8688, "lon": 151.2093, "category": "city"},
    {"name": "Melbourne", "region_code": "AU-VIC", "lat": -37.8136, "lon": 144.9631, "category": "city"},
    {"name": "Cairns", "region_code": "AU-QLD", "lat": -16.9186, "lon": 145.7781, "category": "city"},

    # --- New Zealand (NZ) ---
    {"name": "New Zealand", "region_code": "NZ", "lat": -40.9006, "lon": 174.8860, "category": "hotspot"},
    {"name": "Rotorua", "region_code": "NZ", "lat": -38.1368, "lon": 176.2497, "category": "city"},
    {"name": "Queenstown", "region_code": "NZ", "lat": -45.0312, "lon": 168.6626, "category": "city"},
    {"name": "Milford Sound", "region_code": "NZ", "lat": -44.6716, "lon": 167.9256, "category": "reserve"},
    {"name": "Auckland", "region_code": "NZ", "lat": -36.8485, "lon": 174.7633, "category": "city"},
    {"name": "Christchurch", "region_code": "NZ", "lat": -43.5321, "lon": 172.6362, "category": "city"},

    # --- Japan (JP) ---
    {"name": "Arasaki Crane Sanctuary", "region_code": "JP", "lat": 32.1000, "lon": 130.1833, "category": "hotspot"},
    {"name": "Tokyo", "region_code": "JP", "lat": 35.6762, "lon": 139.6503, "category": "city"},

    # --- Thailand (TH) ---
    {"name": "Khao Yai National Park", "region_code": "TH", "lat": 14.4333, "lon": 101.3667, "category": "reserve"},
    {"name": "Bangkok", "region_code": "TH", "lat": 13.7563, "lon": 100.5018, "category": "city"},
]


# -------------------------------------------------------------------------
# Curated Regional Checklists: Region Code -> Confirmed Taxon IDs
# -------------------------------------------------------------------------
# Local birds for Karnataka (IN-KA)
KARNATAKA_TAXON_IDS = [
    "brakit1",  # Brahminy Kite
    "whtkin2",  # White-throated Kingfisher
    "indcor1",  # Indian Cormorant
    "greegr",   # Great Egret
    "graher1",  # Gray Heron
    "purher1",  # Purple Heron
    "litcor1",  # Little Cormorant
    "darter2",  # Oriental Darter
    "bkwsti",   # Black-winged Stilt
    "rewbul",   # Red-whiskered Bulbul
    "revbul",   # Red-vented Bulbul
    "magrob",   # Oriental Magpie-Robin
    "compea",   # Indian Peafowl
    "asikoe2",  # Asian Koel
    "commyn",   # Common Myna
    "junbab2",  # Jungle Babbler
    "ashpri1",  # Ashy Prinia
    "plapri1",  # Plain Prinia
    "pursun3",  # Purple-rumped Sunbird
    "pursun4",  # Purple Sunbird
    "lobsun2",  # Loten's Sunbird
    "blakit1",  # Black Kite
    "shikra1",  # Shikra
    "crseag1",  # Crested Serpent-Eagle
    "spodov",   # Spotted Dove
    "piekin1",  # Pied Kingfisher
    "comkin1",  # Common Kingfisher
    "grnbee3",  # Asian Green Bee-eater
    "whbwat1",  # White-breasted Waterhen
    "spbpel1",  # Spot-billed Pelican
    "paisto1",  # Painted Stork
    "asiope1",  # Asian Openbill
    "blhibi1",  # Black-headed Ibis
    "brwjac1",  # Bronze-winged Jacana
    "phtjac1",  # Pheasant-tailed Jacana
    "rorpar",   # Rose-ringed Parakeet
    "copbar1",  # Coppersmith Barbet
    "indrob1",  # Indian Robin
    "bladro1",  # Black Drongo
    "whbwag1",  # White-browed Wagtail
    "nutman",   # Scaly-breasted Munia
    "indsil",   # Indian Silverbill
    "baywea1",  # Baya Weaver
    "litegr",   # Little Egret
    "categr2",  # Eastern Cattle-Egret
    "inpher1",  # Indian Pond-Heron
    "bcnher",   # Black-crowned Night Heron
    "grecor",   # Great Cormorant
    "oripip1",  # Paddyfield Pipit
    "rewlap1",  # Red-wattled Lapwing
    "wonsto1",  # Asian Woolly-necked Stork
    "bkrfla1",  # Black-rumped Flameback
    "indrol2",  # Indian Roller
    "stbkin1",  # Stork-billed Kingfisher
    "ruftre2",  # Rufous Treepie
    "maghor2",  # Malabar Gray Hornbill
    "mawthr1",  # Malabar Whistling-Thrush
    "scamin3",  # Orange Minivet
    "brfowl1",  # Brown Fish-Owl
    "tibfly3",  # Tickell's Blue Flycatcher
    "yebbul3",  # Yellow-browed Bulbul
    "putbab1",  # Puff-throated Babbler
    "aspfly1",  # Indian Paradise-Flycatcher
    "isbduc1",  # Indian Spot-billed Duck
    "lewduc1",  # Lesser Whistling-Duck
    "grecou1",  # Greater Coucal
    "junmyn1",  # Jungle Myna
    "purswa3",  # Gray-headed Swamphen
]

# Birds for Rajasthan (IN-RJ)
RAJASTHAN_TAXON_IDS = [
    "compea",   # Indian Peafowl
    "paisto1",  # Painted Stork
    "sarcra1",  # Sarus Crane
    "blnsto1",  # Black-necked Stork
    "eurspo1",  # Eurasian Spoonbill
    "bahgoo",   # Bar-headed Goose
    "gragoo",   # Graylag Goose
    "rudshe",   # Ruddy Shelduck
    "shikra1",  # Shikra
    "crseag1",  # Crested Serpent-Eagle
    "indvul1",  # Indian Vulture
    "whrvul1",  # White-rumped Vulture
    "egyvul1",  # Egyptian Vulture
    "commyn",   # Common Myna
    "banmyn1",  # Bank Myna
    "brasta1",  # Brahminy Starling
    "grefla1",  # Greater Flamingo
    "lesfla1",  # Lesser Flamingo
    "indrol2",  # Indian Roller
    "indbus1",  # Great Indian Bustard
    "demcra1",  # Demoiselle Crane
    "brakit1",  # Brahminy Kite
    "blakit1",  # Black Kite
    "whtkin2",  # White-throated Kingfisher
    "piekin1",  # Pied Kingfisher
    "comkin1",  # Common Kingfisher
    "categr2",  # Eastern Cattle-Egret
    "graher1",  # Gray Heron
    "greegr",   # Great Egret
    "bkwsti",   # Black-winged Stilt
    "bcnher",   # Black-crowned Night Heron
    "spodov",   # Spotted Dove
    "rorpar",   # Rose-ringed Parakeet
    "copbar1",  # Coppersmith Barbet
    "bladro1",  # Black Drongo
    "rewlap1",  # Red-wattled Lapwing
    "wonsto1",  # Asian Woolly-necked Stork
    "bkrfla1",  # Black-rumped Flameback
    "ruftre2",  # Rufous Treepie
    "aspfly1",  # Indian Paradise-Flycatcher
    "junbab2",  # Jungle Babbler
    "grecou1",  # Greater Coucal
]

# Birds for Ladakh (IN-LA)
LADAKH_TAXON_IDS = [
    "blared1",  # Black Redstart
    "chukar",   # Chukar
    "himrub1",  # Himalayan Rubythroat
    "bahgoo",   # Bar-headed Goose
    "bnhgul1",  # Brown-headed Gull
    "rudshe",   # Ruddy Shelduck
    "blncra1",  # Black-necked Crane
    "tibsno1",  # Tibetan Snowcock
    "himsno",   # Himalayan Snowcock
    "tibpar1",  # Tibetan Partridge
    "tibsan1",  # Tibetan Sandgrouse
    "ibisbi1",  # Ibisbill
    "whwred2",  # White-winged Redstart
    "robacc1",  # Robin Accentor
    "broacc1",  # Brown Accentor
    "goleag",   # Golden Eagle
    "lammer1",  # Bearded Vulture
    "rebcho1",  # Red-billed Chough
    "yebcho1",  # Yellow-billed Chough
    "horlar",   # Horned Lark
    "greros1",  # Great Rosefinch
    "comrav",   # Common Raven
    "eurmag1",  # Eurasian Magpie
    "norpin",   # Northern Pintail
    "commer",   # Common Merganser
    "gargan",   # Garganey
    "tufduc",   # Tufted Duck
    "compoc",   # Common Pochard
    "comgre",   # Common Greenshank
    "woosan",   # Wood Sandpiper
    "comsan",   # Common Sandpiper
    "citwag",   # Citrine Wagtail
    "whiwag",   # White Wagtail
    "deswhe1",  # Desert Wheatear
]

# Birds for Florida (US-FL)
FLORIDA_TAXON_IDS = [
    "baleag",   # Bald Eagle
    "osprey",   # Osprey
    "snakit",   # Snail Kite
    "swtkit",   # Swallow-tailed Kite
    "reshaw",   # Red-shouldered Hawk
    "brdowl",   # Barred Owl
    "pilwoo",   # Pileated Woodpecker
    "rebwoo",   # Red-bellied Woodpecker
    "anhing",   # Anhinga
    "doccor",   # Double-crested Cormorant
    "brnpel",   # Brown Pelican
    "amwpel",   # American White Pelican
    "grbher3",  # Great Blue Heron
    "greegr",   # Great Egret
    "snoegr",   # Snowy Egret
    "libher",   # Little Blue Heron
    "triher",   # Tricolored Heron
    "redegr",   # Reddish Egret
    "categr1",  # Western Cattle-Egret
    "grnher",   # Green Heron
    "bcnher",   # Black-crowned Night Heron
    "ycnher",   # Yellow-crowned Night Heron
    "whiibi",   # White Ibis
    "gloibi",   # Glossy Ibis
    "rosspo1",  # Roseate Spoonbill
    "woosto",   # Wood Stork
    "limpki",   # Limpkin
    "purgal2",  # Purple Gallinule
    "comgal1",  # Common Gallinule
    "blkski",   # Black Skimmer
    "laugul",   # Laughing Gull
    "royter1",  # Royal Tern
    "flsjay",   # Florida Scrub-Jay
    "blujay",   # Blue Jay
    "normoc",   # Northern Mockingbird
    "botgra",   # Boat-tailed Grackle
    "comgra",   # Common Grackle
    "rewbla",   # Red-winged Blackbird
    "carwre",   # Carolina Wren
    "norcar",   # Northern Cardinal
    "paibun",   # Painted Bunting
]

# Birds for New York (US-NY)
NEW_YORK_TAXON_IDS = [
    "rethaw",   # Red-tailed Hawk
    "coohaw",   # Cooper's Hawk
    "perfal",   # Peregrine Falcon
    "grbher3",  # Great Blue Heron
    "bcnher",   # Black-crowned Night Heron
    "mallar3",  # Mallard
    "wooduc",   # Wood Duck
    "cangoo",   # Canada Goose
    "norcar",   # Northern Cardinal
    "blujay",   # Blue Jay
    "amerob",   # American Robin
    "sonspa",   # Song Sparrow
    "whtspa",   # White-throated Sparrow
    "daejun",   # Dark-eyed Junco
    "bkcchi",   # Black-capped Chickadee
    "tuftit",   # Tufted Titmouse
    "whbnut",   # White-breasted Nuthatch
    "dowwoo",   # Downy Woodpecker
    "rebwoo",   # Red-bellied Woodpecker
    "eursta",   # European Starling
    "houspa",   # House Sparrow
    "moudov",   # Mourning Dove
    "amegfi",   # American Goldfinch
    "yelwar1",  # Northern Yellow Warbler
    "btbwar",   # Black-throated Blue Warbler
    "amered",   # American Redstart
    "comyel",   # Common Yellowthroat
]

# Birds for South Africa (ZA)
SOUTH_AFRICA_TAXON_IDS = [
    "ostric2",  # Common Ostrich
    "jacpen1",  # African Penguin
    "capgan1",  # Cape Gannet
    "capcor1",  # Cape Cormorant
    "affeag1",  # African Fish-Eagle
    "mareag1",  # Martial Eagle
    "batele1",  # Bateleur
    "taweag1",  # Tawny Eagle
    "secret2",  # Secretarybird
    "korbus1",  # Kori Bustard
    "helgui",   # Helmeted Guineafowl
    "marsto1",  # Marabou Stork
    "sabsto1",  # Saddle-billed Stork
    "yebsto1",  # Yellow-billed Stork
    "hamerk1",  # Hamerkop
    "hadibi1",  # Hadada Ibis
    "sacibi2",  # African Sacred Ibis
    "egygoo",   # Egyptian Goose
    "spwgoo1",  # Spur-winged Goose
    "librol2",  # Lilac-breasted Roller
    "sybhor1",  # Southern Yellow-billed Hornbill
    "srbhor1",  # Southern Red-billed Hornbill
    "reboxp1",  # Red-billed Oxpecker
    "yeboxp1",  # Yellow-billed Oxpecker
    "carcha1",  # Cape Robin-Chat
    "capwag1",  # Cape Wagtail
    "afpwag1",  # African Pied Wagtail
    "capsug1",  # Cape Sugarbird
    "orbsun2",  # Orange-breasted Sunbird
    "malsun1",  # Malachite Sunbird
    "sdcsun3",  # Southern Double-collared Sunbird
    "capwea1",  # Cape Weaver
    "afmwea",   # Southern Masked-Weaver
    "rebque1",  # Red-billed Quelea
    "blucra2",  # Blue Crane
    "grccra1",  # Gray Crowned-Crane
    "piekin1",  # Pied Kingfisher
    "giakin1",  # Giant Kingfisher
    "malkin1",  # Malachite Kingfisher
    "knytur1",  # Knysna Turaco
]

# Birds for Kenya (KE / KE-01)
KENYA_TAXON_IDS = [
    "librol2",  # Lilac-breasted Roller
    "supsta1",  # Superb Starling
    "affeag1",  # African Fish-Eagle
    "mareag1",  # Martial Eagle
    "secret2",  # Secretarybird
    "korbus1",  # Kori Bustard
    "helgui",   # Helmeted Guineafowl
    "marsto1",  # Marabou Stork
    "sabsto1",  # Saddle-billed Stork
    "hamerk1",  # Hamerkop
    "hadibi1",  # Hadada Ibis
    "egygoo",   # Egyptian Goose
    "spwgoo1",  # Spur-winged Goose
    "ostric2",  # Common Ostrich
    "ostric3",  # Somali Ostrich
    "lesfla1",  # Lesser Flamingo
    "grefla1",  # Greater Flamingo
    "grwpel1",  # Great White Pelican
    "pibpel1",  # Pink-backed Pelican
    "grccra1",  # Gray Crowned-Crane
    "reboxp1",  # Red-billed Oxpecker
    "yeboxp1",  # Yellow-billed Oxpecker
    "afpwag1",  # African Pied Wagtail
    "taweag1",  # Tawny Eagle
    "batele1",  # Bateleur
]

# Birds for United Kingdom (GB)
UK_TAXON_IDS = [
    "mallar3",  # Mallard
    "grecor",   # Great Cormorant
    "eurmag1",  # Eurasian Magpie
    "eursta",   # European Starling
    "houspa",   # House Sparrow
    "comrav",   # Common Raven
    "eurrob1",  # European Robin
    "sonthr1",  # Song Thrush
    "blutit",   # Eurasian Blue Tit
    "gretit1",  # Great Tit
    "comcha",   # Common Chaffinch
    "comchi1",  # Common Chiffchaff
    "eurbla",   # Eurasian Blackbird
    "comkin1",  # Common Kingfisher
    "graher1",  # Gray Heron
    "osprey",   # Osprey
    "perfal",   # Peregrine Falcon
    "bahgoo",   # Bar-headed Goose
]

# Birds for New Zealand (NZ)
NEW_ZEALAND_TAXON_IDS = [
    "kea1",     # Kea
    "nezpig2",  # New Zealand Pigeon
    "nezsca1",  # New Zealand Scaup
    "weka1",    # Weka
    "sobkiw1",  # Southern Brown Kiwi
    "liskiw1",  # Little Spotted Kiwi
    "takahe3",  # South Island Takahe
    "nezkak1",  # New Zealand Kaka
    "blkswa",   # Black Swan
    "mallar3",  # Mallard
    "parshe1",  # Paradise Shelduck
    "purswa3",  # Gray-headed Swamphen
    "welswa1",  # Welcome Swallow
    "piecor1",  # Pied Cormorant
    "lipcor1",  # Little Pied Cormorant
    "soioys1",  # South Island Oystercatcher
    "varoys1",  # Variable Oystercatcher
    "swahar1",  # Swamp Harrier
    "nezbel1",  # New Zealand Bellbird
    "nezfan1",  # New Zealand Fantail
    "ausgan1",  # Australasian Gannet
]

# Map of regional codes to taxon ID lists
REGIONAL_CHECKLISTS_MAP: Dict[str, List[str]] = {
    "IN-KA": KARNATAKA_TAXON_IDS,
    "IN-RJ": RAJASTHAN_TAXON_IDS,
    "IN-LA": LADAKH_TAXON_IDS,
    "IN-GJ": RAJASTHAN_TAXON_IDS + KARNATAKA_TAXON_IDS[:25],
    "IN-UT": RAJASTHAN_TAXON_IDS + LADAKH_TAXON_IDS[:10],
    "IN-AS": KARNATAKA_TAXON_IDS[:30],
    "IN-WB": KARNATAKA_TAXON_IDS[:35],
    "IN-KL": KARNATAKA_TAXON_IDS,
    "IN-TN": KARNATAKA_TAXON_IDS,
    "IN-MH": KARNATAKA_TAXON_IDS,
    "IN-DL": RAJASTHAN_TAXON_IDS,
    "IN-HR": RAJASTHAN_TAXON_IDS,
    "US-FL": FLORIDA_TAXON_IDS,
    "US-NY": NEW_YORK_TAXON_IDS,
    "US-CA": NEW_YORK_TAXON_IDS[:15] + FLORIDA_TAXON_IDS[:15],
    "US-TX": FLORIDA_TAXON_IDS[:25] + NEW_YORK_TAXON_IDS[:15],
    "US-AZ": NEW_YORK_TAXON_IDS[:15],
    "US-AK": LADAKH_TAXON_IDS[:15] + NEW_YORK_TAXON_IDS[:10],
    "US-WA": NEW_YORK_TAXON_IDS[:20],
    "US-CO": NEW_YORK_TAXON_IDS[:20],
    "ZA": SOUTH_AFRICA_TAXON_IDS,
    "KE": KENYA_TAXON_IDS,
    "KE-01": KENYA_TAXON_IDS,
    "TZ": KENYA_TAXON_IDS,
    "BW": SOUTH_AFRICA_TAXON_IDS,
    "GB": UK_TAXON_IDS,
    "NZ": NEW_ZEALAND_TAXON_IDS,
}


def build_gazetteer(taxonomy_path: str = TAXONOMY_PATH) -> None:
    """Validates and writes data/gazetteer.json and data/regional_checklists.json."""
    if not os.path.exists(taxonomy_path):
        raise FileNotFoundError(f"Taxonomy file not found at {taxonomy_path}")

    with open(taxonomy_path, "r", encoding="utf-8") as f:
        taxonomy = json.load(f)

    valid_taxon_ids: Set[str] = {s["taxon_id"] for s in taxonomy}
    print(f"Loaded {len(valid_taxon_ids)} valid taxon IDs from {taxonomy_path}")

    # 1. Prepare Gazetteer
    gazetteer: List[Dict[str, Any]] = []
    seen_names = set()
    for entry in RAW_GAZETTEER:
        name = entry["name"]
        region = entry["region_code"]
        lat = float(entry["lat"])
        lon = float(entry["lon"])
        cat = entry["category"]

        assert -90.0 <= lat <= 90.0, f"Invalid latitude {lat} for {name}"
        assert -180.0 <= lon <= 180.0, f"Invalid longitude {lon} for {name}"
        assert cat in {"hotspot", "city", "reserve"}, f"Invalid category {cat} for {name}"

        # Populate both 'lat'/'lon' and 'latitude'/'longitude' for full cross-system compatibility
        item = {
            "name": name,
            "region_code": region,
            "lat": lat,
            "lon": lon,
            "latitude": lat,
            "longitude": lon,
            "category": cat,
        }
        gazetteer.append(item)
        seen_names.add(name.lower())

    # Verify key hotspots are present
    assert "saul kere" in seen_names or "saul kere lake" in seen_names
    assert "bengaluru" in seen_names or "bangalore" in seen_names
    assert "ranthambore" in seen_names or "ranthambore national park" in seen_names
    assert "leh" in seen_names
    assert "everglades" in seen_names or "everglades national park" in seen_names
    assert "kruger" in seen_names or "kruger national park" in seen_names
    assert "central park" in seen_names

    # 2. Prepare Regional Checklists
    checklists: Dict[str, List[str]] = {}
    for region_code, tids in REGIONAL_CHECKLISTS_MAP.items():
        # Deduplicate while preserving order
        unique_tids = []
        seen = set()
        for tid in tids:
            if tid not in valid_taxon_ids:
                raise ValueError(f"Taxon ID '{tid}' for region '{region_code}' not found in taxonomy.json")
            if tid not in seen:
                seen.add(tid)
                unique_tids.append(tid)

        checklists[region_code] = unique_tids

    # Verify Karnataka (IN-KA) invariants
    ka_set = set(checklists["IN-KA"])
    assert "brakit1" in ka_set, "Brahminy Kite (brakit1) must be in IN-KA"
    assert "whtkin2" in ka_set, "White-throated Kingfisher (whtkin2) must be in IN-KA"
    assert "indcor1" in ka_set, "Indian Cormorant (indcor1) must be in IN-KA"
    assert "greegr" in ka_set, "Great Egret (greegr) must be in IN-KA"
    assert "graher1" in ka_set, "Gray Heron (graher1) must be in IN-KA"
    assert "baleag" not in ka_set, "Bald Eagle (baleag) must not be in IN-KA"
    assert "blkswa" not in ka_set, "Black Swan (blkswa) must not be in IN-KA"

    # Verify Florida (US-FL) has Bald Eagle
    assert "baleag" in checklists["US-FL"], "Bald Eagle (baleag) must be in US-FL"

    # Write outputs
    os.makedirs(DATA_DIR, exist_ok=True)

    with open(GAZETTEER_PATH, "w", encoding="utf-8") as f:
        json.dump(gazetteer, f, indent=2, ensure_ascii=False)
    print(f"Successfully wrote {len(gazetteer)} gazetteer entries to {GAZETTEER_PATH}")

    with open(CHECKLISTS_PATH, "w", encoding="utf-8") as f:
        json.dump(checklists, f, indent=2, ensure_ascii=False)
    print(f"Successfully wrote {len(checklists)} regional checklists to {CHECKLISTS_PATH}")


if __name__ == "__main__":
    build_gazetteer()
