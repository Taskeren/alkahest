cbuffer cb_entid : register(b0) {
    uint entity_id;
}

uint PSMain() : SV_Target0 {
    return entity_id;
}