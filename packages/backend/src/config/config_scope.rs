use actix_web::web;

use crate::api::routes::{
    confirm_deposit, get_intent_status, get_metrics, health_check, health_check_detailed,
    indexer_event, initiate_bridge, list_intents, root,
};

pub fn configure(conf: &mut web::ServiceConfig) {
    let scope = web::scope("/api/v1")
        .service(initiate_bridge)
        .service(confirm_deposit)
        .service(get_intent_status)
        .service(list_intents)
        .service(indexer_event)
        .service(get_metrics)
        .service(health_check)
        .service(health_check_detailed)
        .service(root);

    conf.service(scope);
}
