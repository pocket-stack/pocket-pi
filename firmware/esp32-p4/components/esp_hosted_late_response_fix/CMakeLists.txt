idf_component_register(PRIV_REQUIRES espressif__esp_hosted)

idf_component_get_property(esp_hosted_dir espressif__esp_hosted COMPONENT_DIR)
set(rpc_core "${esp_hosted_dir}/host/drivers/rpc/core/rpc_core.c")
file(READ "${rpc_core}" source)

set(before [=[
			if (g_h.funcs->_h_queue_item(rpc_rx_q, &elem, HOSTED_BLOCK_MAX)) {
				ESP_LOGE(TAG, "RPC Q put fail");
				goto free_buffers;
			}

			/* Call up rx ind to unblock caller */
			if (CALLBACK_AVAILABLE == is_sync_resp_sem_available(app_resp->uid))
				post_sync_resp_sem(app_resp);
]=])
set(after [=[
			if (CALLBACK_AVAILABLE != is_sync_resp_sem_available(app_resp->uid)) {
				ESP_LOGW(TAG, "Dropping late sync response [0x%x], uid %ld",
						app_resp->msg_id, app_resp->uid);
				goto free_buffers;
			}

			if (g_h.funcs->_h_queue_item(rpc_rx_q, &elem, HOSTED_BLOCK_MAX)) {
				ESP_LOGE(TAG, "RPC Q put fail");
				goto free_buffers;
			}

			/* Call up rx ind to unblock caller */
			post_sync_resp_sem(app_resp);
]=])

string(FIND "${source}" "Dropping late sync response" already_patched)
if(already_patched EQUAL -1)
    string(FIND "${source}" "${before}" patch_site)
    if(patch_site EQUAL -1)
        message(FATAL_ERROR "esp_hosted RPC response path no longer matches the pinned patch")
    endif()
    string(REPLACE "${before}" "${after}" source "${source}")
    file(WRITE "${rpc_core}" "${source}")
endif()
