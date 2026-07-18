CREATE TYPE seller_verification AS ENUM (
    'verified',
    'reported',
    'unknown'
);

CREATE TYPE listing_category AS ENUM (
    'mobile_phones',
    'tablets',
    'laptops',
    'computers',
    'accessories',
    'cameras',
    'tv_and_video',
    'audio',
    'gaming',
    'appliances',
    'furniture',
    'clothing',
    'shoes',
    'watches',
    'jewellery',
    'vehicles',
    'vehicle_parts',
    'property',
    'books',
    'sports',
    'toys',
    'tools',
    'services',
    'other'
);

CREATE TYPE report_types AS ENUM (
    'scam',
    'fake_item',
    'no_delivery',
    'wrong_item',
    'counterfeit',
    'non_responsive'
);

CREATE TYPE risk_level_type AS ENUM (
    'low',
    'caution',
    'high'
);
