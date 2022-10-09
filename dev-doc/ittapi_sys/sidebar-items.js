initSidebarItems({"constant":[["ITT_MAJOR",""],["ITT_MINOR",""],["ITT_OS",""],["ITT_OS_FREEBSD",""],["ITT_OS_LINUX",""],["ITT_OS_MAC",""],["ITT_OS_WIN",""],["ITT_PLATFORM",""],["ITT_PLATFORM_FREEBSD",""],["ITT_PLATFORM_MAC",""],["ITT_PLATFORM_POSIX",""],["ITT_PLATFORM_WIN",""],["___itt_track_group_type___itt_track_group_type_normal",""],["___itt_track_type___itt_track_type_normal",""],["__itt_attr_barrier",""],["__itt_attr_mutex",""],["__itt_heap_growth",""],["__itt_heap_leaks",""],["__itt_metadata_type___itt_metadata_double","< SIgned 64-bit floating-point"],["__itt_metadata_type___itt_metadata_float","< Signed 32-bit floating-point"],["__itt_metadata_type___itt_metadata_s16","< Signed 16-bit integer"],["__itt_metadata_type___itt_metadata_s32","< Signed 32-bit integer"],["__itt_metadata_type___itt_metadata_s64","< Signed 64-bit integer"],["__itt_metadata_type___itt_metadata_u16","< Unsigned 16-bit integer"],["__itt_metadata_type___itt_metadata_u32","< Unsigned 32-bit integer"],["__itt_metadata_type___itt_metadata_u64","< Unsigned 64-bit integer"],["__itt_metadata_type___itt_metadata_unknown",""],["__itt_model_disable___itt_model_disable_collection",""],["__itt_model_disable___itt_model_disable_observation",""],["__itt_module_type___itt_module_type_coff",""],["__itt_module_type___itt_module_type_elf",""],["__itt_module_type___itt_module_type_unknown",""],["__itt_relation___itt_relation_is_child_of","< “A is child of B” means that A was created by B (inverse of is_parent_of)"],["__itt_relation___itt_relation_is_continuation_of","< “A is continuation of B” means that A assumes the dependencies of B"],["__itt_relation___itt_relation_is_continued_by","< “A is continued by B” means that B assumes the dependencies of A (inverse of is_continuation_of)"],["__itt_relation___itt_relation_is_dependent_on","< “A is dependent on B” means that A cannot start until B completes"],["__itt_relation___itt_relation_is_parent_of","< “A is parent of B” means that A created B"],["__itt_relation___itt_relation_is_predecessor_to","< “A is predecessor to B” means that B cannot start until A completes (inverse of is_dependent_on)"],["__itt_relation___itt_relation_is_sibling_of","< “A is sibling of B” means that A and B were created as a group"],["__itt_relation___itt_relation_is_unknown",""],["__itt_scope___itt_scope_global",""],["__itt_scope___itt_scope_marker",""],["__itt_scope___itt_scope_task",""],["__itt_scope___itt_scope_track",""],["__itt_scope___itt_scope_track_group",""],["__itt_scope___itt_scope_unknown",""],["__itt_section_exec",""],["__itt_section_read",""],["__itt_section_type_itt_section_type_bss",""],["__itt_section_type_itt_section_type_data",""],["__itt_section_type_itt_section_type_text",""],["__itt_section_type_itt_section_type_unknown",""],["__itt_section_write",""],["__itt_suppress_all_errors",""],["__itt_suppress_memory_errors",""],["__itt_suppress_mode___itt_suppress_range",""],["__itt_suppress_mode___itt_unsuppress_range",""],["__itt_suppress_threading_errors",""],["_iJIT_CodeArchitecture_iJIT_CA_32","<\\brief 32-bit machine code."],["_iJIT_CodeArchitecture_iJIT_CA_64","<\\brief 64-bit machine code."],["_iJIT_CodeArchitecture_iJIT_CA_NATIVE","<\\brief Native to the process architecture that is calling it."],["_iJIT_IsProfilingActiveFlags_iJIT_NOTHING_RUNNING","<\\brief The agent is not running; iJIT_NotifyEvent calls will not be processed."],["_iJIT_IsProfilingActiveFlags_iJIT_SAMPLING_ON","<\\brief The agent is running and ready to process notifications."],["_iJIT_SegmentType_iJIT_CT_CODE","<\\brief Executable code."],["_iJIT_SegmentType_iJIT_CT_DATA","<\\brief Data (not executable code). VTune Amplifier uses the format string (see iJIT_Method_Update) to represent this data in the VTune Amplifier GUI"],["_iJIT_SegmentType_iJIT_CT_EOF",""],["_iJIT_SegmentType_iJIT_CT_KEEP","<\\brief Use the previous markup for the trace. Can be used for the following iJVM_EVENT_TYPE_METHOD_UPDATE_V2 events, if the type of the previously reported segment type is the same."],["_iJIT_SegmentType_iJIT_CT_UNKNOWN",""],["iJIT_jvm_event_iJVM_EVENT_TYPE_METHOD_INLINE_LOAD_FINISHED","<\\brief Send when an inline dynamic code is JIT compiled and loaded into memory by the JIT engine, but before the parent code region starts executing. Use iJIT_Method_Inline_Load as event data."],["iJIT_jvm_event_iJVM_EVENT_TYPE_METHOD_LOAD_FINISHED","<\\brief Send when dynamic code is JIT compiled and loaded into memory by the JIT engine, but before the code is executed. Use iJIT_Method_Load as event data."],["iJIT_jvm_event_iJVM_EVENT_TYPE_METHOD_LOAD_FINISHED_V2","<\\brief Send when a dynamic code is JIT compiled and loaded into memory by the JIT engine, but before the code is executed. Use iJIT_Method_Load_V2 as event data."],["iJIT_jvm_event_iJVM_EVENT_TYPE_METHOD_LOAD_FINISHED_V3","<\\brief Send when a dynamic code is JIT compiled and loaded into memory by the JIT engine, but before the code is executed. Use iJIT_Method_Load_V3 as event data."],["iJIT_jvm_event_iJVM_EVENT_TYPE_METHOD_UNLOAD_START","<\\brief Send when compiled dynamic code is being unloaded from memory. Use iJIT_Method_Load as event data."],["iJIT_jvm_event_iJVM_EVENT_TYPE_METHOD_UPDATE","<\\brief Send to provide new content for a previously reported dynamic code. The previous content will be invalidated starting from the time of the notification. Use iJIT_Method_Load as event data but required fields are following:"],["iJIT_jvm_event_iJVM_EVENT_TYPE_METHOD_UPDATE_V2","@cond exclude_from_documentation"],["iJIT_jvm_event_iJVM_EVENT_TYPE_SHUTDOWN","<\\brief Send this to shutdown the agent. Use NULL for event data."]],"fn":[["iJIT_GetNewMethodID","@brief Generates a new unique method ID."],["iJIT_IsProfilingActive","@brief Returns the current mode of the agent."],["iJIT_NotifyEvent","@brief Reports infomation about JIT-compiled code to the agent."]],"static":[["__itt_av_save_ptr__3_0",""],["__itt_clock_domain_create_ptr__3_0",""],["__itt_clock_domain_reset_ptr__3_0",""],["__itt_counter_create_ptr__3_0",""],["__itt_counter_create_typed_ptr__3_0",""],["__itt_counter_dec_delta_ptr__3_0",""],["__itt_counter_dec_delta_v3_ptr__3_0",""],["__itt_counter_dec_ptr__3_0",""],["__itt_counter_dec_v3_ptr__3_0",""],["__itt_counter_destroy_ptr__3_0",""],["__itt_counter_inc_delta_ptr__3_0",""],["__itt_counter_inc_delta_v3_ptr__3_0",""],["__itt_counter_inc_ptr__3_0",""],["__itt_counter_inc_v3_ptr__3_0",""],["__itt_counter_set_value_ex_ptr__3_0",""],["__itt_counter_set_value_ptr__3_0",""],["__itt_detach_ptr__3_0",""],["__itt_domain_create_ptr__3_0",""],["__itt_enable_attach_ptr__3_0",""],["__itt_event_create_ptr__3_0",""],["__itt_event_end_ptr__3_0",""],["__itt_event_start_ptr__3_0",""],["__itt_frame_begin_v3_ptr__3_0",""],["__itt_frame_end_v3_ptr__3_0",""],["__itt_frame_submit_v3_ptr__3_0",""],["__itt_fsync_acquired_ptr__3_0",""],["__itt_fsync_cancel_ptr__3_0",""],["__itt_fsync_prepare_ptr__3_0",""],["__itt_fsync_releasing_ptr__3_0",""],["__itt_get_timestamp_ptr__3_0",""],["__itt_heap_allocate_begin_ptr__3_0",""],["__itt_heap_allocate_end_ptr__3_0",""],["__itt_heap_free_begin_ptr__3_0",""],["__itt_heap_free_end_ptr__3_0",""],["__itt_heap_function_create_ptr__3_0",""],["__itt_heap_internal_access_begin_ptr__3_0",""],["__itt_heap_internal_access_end_ptr__3_0",""],["__itt_heap_reallocate_begin_ptr__3_0",""],["__itt_heap_reallocate_end_ptr__3_0",""],["__itt_heap_record_memory_growth_begin_ptr__3_0",""],["__itt_heap_record_memory_growth_end_ptr__3_0",""],["__itt_heap_record_ptr__3_0",""],["__itt_heap_reset_detection_ptr__3_0",""],["__itt_histogram_create_ptr__3_0",""],["__itt_histogram_submit_ptr__3_0",""],["__itt_id_create_ex_ptr__3_0",""],["__itt_id_create_ptr__3_0",""],["__itt_id_destroy_ex_ptr__3_0",""],["__itt_id_destroy_ptr__3_0",""],["__itt_marker_ex_ptr__3_0",""],["__itt_marker_ptr__3_0",""],["__itt_metadata_add_ptr__3_0",""],["__itt_metadata_add_with_scope_ptr__3_0",""],["__itt_metadata_str_add_ptr__3_0",""],["__itt_metadata_str_add_with_scope_ptr__3_0",""],["__itt_model_aggregate_task_ptr__3_0",""],["__itt_model_clear_uses_ptr__3_0",""],["__itt_model_disable_pop_ptr__3_0",""],["__itt_model_disable_push_ptr__3_0",""],["__itt_model_induction_uses_ptr__3_0",""],["__itt_model_iteration_taskAL_ptr__3_0",""],["__itt_model_iteration_taskA_ptr__3_0",""],["__itt_model_lock_acquire_2_ptr__3_0",""],["__itt_model_lock_acquire_ptr__3_0",""],["__itt_model_lock_release_2_ptr__3_0",""],["__itt_model_lock_release_ptr__3_0",""],["__itt_model_observe_uses_ptr__3_0",""],["__itt_model_record_allocation_ptr__3_0",""],["__itt_model_record_deallocation_ptr__3_0",""],["__itt_model_reduction_uses_ptr__3_0",""],["__itt_model_site_beginAL_ptr__3_0",""],["__itt_model_site_beginA_ptr__3_0",""],["__itt_model_site_begin_ptr__3_0",""],["__itt_model_site_end_2_ptr__3_0",""],["__itt_model_site_end_ptr__3_0",""],["__itt_model_task_beginAL_ptr__3_0",""],["__itt_model_task_beginA_ptr__3_0",""],["__itt_model_task_begin_ptr__3_0",""],["__itt_model_task_end_2_ptr__3_0",""],["__itt_model_task_end_ptr__3_0",""],["__itt_module_load_ptr__3_0",""],["__itt_module_load_with_sections_ptr__3_0",""],["__itt_module_unload_ptr__3_0",""],["__itt_module_unload_with_sections_ptr__3_0",""],["__itt_null","@endcond"],["__itt_pause_ptr__3_0",""],["__itt_pt_region_create_ptr__3_0",""],["__itt_region_begin_ptr__3_0",""],["__itt_region_end_ptr__3_0",""],["__itt_relation_add_ex_ptr__3_0",""],["__itt_relation_add_ptr__3_0",""],["__itt_relation_add_to_current_ex_ptr__3_0",""],["__itt_relation_add_to_current_ptr__3_0",""],["__itt_resume_ptr__3_0",""],["__itt_set_track_ptr__3_0",""],["__itt_string_handle_create_ptr__3_0",""],["__itt_suppress_clear_range_ptr__3_0",""],["__itt_suppress_mark_range_ptr__3_0",""],["__itt_suppress_pop_ptr__3_0",""],["__itt_suppress_push_ptr__3_0",""],["__itt_sync_acquired_ptr__3_0",""],["__itt_sync_cancel_ptr__3_0",""],["__itt_sync_create_ptr__3_0",""],["__itt_sync_destroy_ptr__3_0",""],["__itt_sync_prepare_ptr__3_0",""],["__itt_sync_releasing_ptr__3_0",""],["__itt_sync_rename_ptr__3_0",""],["__itt_task_begin_ex_ptr__3_0",""],["__itt_task_begin_fn_ex_ptr__3_0",""],["__itt_task_begin_fn_ptr__3_0",""],["__itt_task_begin_overlapped_ptr__3_0",""],["__itt_task_begin_ptr__3_0",""],["__itt_task_end_ex_ptr__3_0",""],["__itt_task_end_overlapped_ptr__3_0",""],["__itt_task_end_ptr__3_0",""],["__itt_task_group_ptr__3_0",""],["__itt_thread_ignore_ptr__3_0",""],["__itt_thread_set_name_ptr__3_0",""],["__itt_track_create_ptr__3_0",""],["__itt_track_group_create_ptr__3_0",""]],"struct":[["_LineNumberInfo","@brief Description of a single entry in the line number information of a code region. @details A table of line number entries gives information about how the reported code region is mapped to source file. Intel(R) VTune(TM) Amplifier uses line number information to attribute the samples (virtual address) to a line number. \\n It is acceptable to report different code addresses for the same source line: @code Offset LineNumber 1       2 12      4 15      2 18      1 21      30"],["___itt_clock_domain",""],["___itt_clock_info",""],["___itt_counter",""],["___itt_domain",""],["___itt_histogram",""],["___itt_id",""],["___itt_module_object",""],["___itt_section_info",""],["___itt_string_handle",""],["___itt_track",""],["___itt_track_group",""],["_iJIT_Method_Inline_Load","@brief Description of an inline JIT-compiled method @details When you use the_iJIT_Method_Inline_Load structure to describe the JIT compiled method, use iJVM_EVENT_TYPE_METHOD_INLINE_LOAD_FINISHED as an event type to report it."],["_iJIT_Method_Load","@brief Description of a JIT-compiled method @details When you use the iJIT_Method_Load structure to describe the JIT compiled method, use iJVM_EVENT_TYPE_METHOD_LOAD_FINISHED as an event type to report it."],["_iJIT_Method_Load_V2","@brief Description of a JIT-compiled method @details When you use the iJIT_Method_Load_V2 structure to describe the JIT compiled method, use iJVM_EVENT_TYPE_METHOD_LOAD_FINISHED_V2 as an event type to report it."],["_iJIT_Method_Load_V3","@brief Description of a JIT-compiled method @details The iJIT_Method_Load_V3 structure is the same as iJIT_Method_Load_V2 with a newly introduced ‘arch’ field that specifies architecture of the code region. When you use the iJIT_Method_Load_V3 structure to describe the JIT compiled method, use iJVM_EVENT_TYPE_METHOD_LOAD_FINISHED_V3 as an event type to report it."],["_iJIT_Method_Update","@brief Description of a dynamic update of the content within JIT-compiled method @details The JIT engine may generate the methods that are updated at runtime partially by mixed (data + executable code) content. When you use the iJIT_Method_Update structure to describe the update of the content within a JIT-compiled method, use iJVM_EVENT_TYPE_METHOD_UPDATE_V2 as an event type to report it."]],"type":[["LineNumberInfo","@brief Description of a single entry in the line number information of a code region. @details A table of line number entries gives information about how the reported code region is mapped to source file. Intel(R) VTune(TM) Amplifier uses line number information to attribute the samples (virtual address) to a line number. \\n It is acceptable to report different code addresses for the same source line: @code Offset LineNumber 1       2 12      4 15      2 18      1 21      30"],["___itt_track_group_type","@cond exclude_from_documentation"],["___itt_track_type","@brief Placeholder for custom track types. Currently, “normal” custom track is the only available track type."],["__itt_av_save_ptr__3_0_t",""],["__itt_clock_domain",""],["__itt_clock_domain_create_ptr__3_0_t",""],["__itt_clock_domain_reset_ptr__3_0_t",""],["__itt_clock_info",""],["__itt_counter","@brief opaque structure for counter identification"],["__itt_counter_create_ptr__3_0_t",""],["__itt_counter_create_typed_ptr__3_0_t",""],["__itt_counter_dec_delta_ptr__3_0_t",""],["__itt_counter_dec_delta_v3_ptr__3_0_t",""],["__itt_counter_dec_ptr__3_0_t",""],["__itt_counter_dec_v3_ptr__3_0_t",""],["__itt_counter_destroy_ptr__3_0_t",""],["__itt_counter_inc_delta_ptr__3_0_t",""],["__itt_counter_inc_delta_v3_ptr__3_0_t",""],["__itt_counter_inc_ptr__3_0_t",""],["__itt_counter_inc_v3_ptr__3_0_t",""],["__itt_counter_set_value_ex_ptr__3_0_t",""],["__itt_counter_set_value_ptr__3_0_t",""],["__itt_detach_ptr__3_0_t",""],["__itt_domain",""],["__itt_domain_create_ptr__3_0_t",""],["__itt_enable_attach_ptr__3_0_t",""],["__itt_event","@cond exclude_from_gpa_documentation */ @defgroup events Events @ingroup public Events group @{"],["__itt_event_create_ptr__3_0_t",""],["__itt_event_end_ptr__3_0_t",""],["__itt_event_start_ptr__3_0_t",""],["__itt_frame_begin_v3_ptr__3_0_t",""],["__itt_frame_end_v3_ptr__3_0_t",""],["__itt_frame_submit_v3_ptr__3_0_t",""],["__itt_fsync_acquired_ptr__3_0_t",""],["__itt_fsync_cancel_ptr__3_0_t",""],["__itt_fsync_prepare_ptr__3_0_t",""],["__itt_fsync_releasing_ptr__3_0_t",""],["__itt_get_clock_info_fn","@cond exclude_from_documentation"],["__itt_get_timestamp_ptr__3_0_t",""],["__itt_heap_allocate_begin_ptr__3_0_t",""],["__itt_heap_allocate_end_ptr__3_0_t",""],["__itt_heap_free_begin_ptr__3_0_t",""],["__itt_heap_free_end_ptr__3_0_t",""],["__itt_heap_function","@defgroup heap Heap @ingroup public Heap group @{"],["__itt_heap_function_create_ptr__3_0_t",""],["__itt_heap_internal_access_begin_ptr__3_0_t",""],["__itt_heap_internal_access_end_ptr__3_0_t",""],["__itt_heap_reallocate_begin_ptr__3_0_t",""],["__itt_heap_reallocate_end_ptr__3_0_t",""],["__itt_heap_record_memory_growth_begin_ptr__3_0_t",""],["__itt_heap_record_memory_growth_end_ptr__3_0_t",""],["__itt_heap_record_ptr__3_0_t",""],["__itt_heap_reset_detection_ptr__3_0_t",""],["__itt_histogram",""],["__itt_histogram_create_ptr__3_0_t",""],["__itt_histogram_submit_ptr__3_0_t",""],["__itt_id",""],["__itt_id_create_ex_ptr__3_0_t",""],["__itt_id_create_ptr__3_0_t",""],["__itt_id_destroy_ex_ptr__3_0_t",""],["__itt_id_destroy_ptr__3_0_t",""],["__itt_marker_ex_ptr__3_0_t",""],["__itt_marker_ptr__3_0_t",""],["__itt_metadata_add_ptr__3_0_t",""],["__itt_metadata_add_with_scope_ptr__3_0_t",""],["__itt_metadata_str_add_ptr__3_0_t",""],["__itt_metadata_str_add_with_scope_ptr__3_0_t",""],["__itt_metadata_type","@ingroup parameters @brief describes the type of metadata"],["__itt_model_aggregate_task_ptr__3_0_t",""],["__itt_model_clear_uses_ptr__3_0_t",""],["__itt_model_disable","@enum __itt_model_disable @brief Enumerator for the disable methods"],["__itt_model_disable_pop_ptr__3_0_t",""],["__itt_model_disable_push_ptr__3_0_t",""],["__itt_model_induction_uses_ptr__3_0_t",""],["__itt_model_iteration_taskAL_ptr__3_0_t",""],["__itt_model_iteration_taskA_ptr__3_0_t",""],["__itt_model_lock_acquire_2_ptr__3_0_t",""],["__itt_model_lock_acquire_ptr__3_0_t",""],["__itt_model_lock_release_2_ptr__3_0_t",""],["__itt_model_lock_release_ptr__3_0_t",""],["__itt_model_observe_uses_ptr__3_0_t",""],["__itt_model_record_allocation_ptr__3_0_t",""],["__itt_model_record_deallocation_ptr__3_0_t",""],["__itt_model_reduction_uses_ptr__3_0_t",""],["__itt_model_site",""],["__itt_model_site_beginAL_ptr__3_0_t",""],["__itt_model_site_beginA_ptr__3_0_t",""],["__itt_model_site_begin_ptr__3_0_t",""],["__itt_model_site_end_2_ptr__3_0_t",""],["__itt_model_site_end_ptr__3_0_t",""],["__itt_model_site_instance",""],["__itt_model_task",""],["__itt_model_task_beginAL_ptr__3_0_t",""],["__itt_model_task_beginA_ptr__3_0_t",""],["__itt_model_task_begin_ptr__3_0_t",""],["__itt_model_task_end_2_ptr__3_0_t",""],["__itt_model_task_end_ptr__3_0_t",""],["__itt_model_task_instance",""],["__itt_module_load_ptr__3_0_t",""],["__itt_module_load_with_sections_ptr__3_0_t",""],["__itt_module_object",""],["__itt_module_type","@cond exclude_from_documentation"],["__itt_module_unload_ptr__3_0_t",""],["__itt_module_unload_with_sections_ptr__3_0_t",""],["__itt_pause_ptr__3_0_t",""],["__itt_pt_region","@defgroup Intel Processor Trace control API from this group provides control over collection and analysis of Intel Processor Trace (Intel PT) data Information about Intel Processor Trace technology can be found here (Volume 3 chapter 35): https://software.intel.com/sites/default/files/managed/39/c5/325462-sdm-vol-1-2abcd-3abcd.pdf Use this API to mark particular code regions for loading detailed performance statistics. This mode makes your analysis faster and more accurate. @{"],["__itt_pt_region_create_ptr__3_0_t",""],["__itt_region_begin_ptr__3_0_t",""],["__itt_region_end_ptr__3_0_t",""],["__itt_relation","@ingroup relations @brief The kind of relation between two instances is specified by the enumerated type __itt_relation. Relations between instances can be added with an API call. The relation API uses instance IDs. Relations can be added before or after the actual instances are created and persist independently of the instances. This is the motivation for having different lifetimes for instance IDs and the actual instances."],["__itt_relation_add_ex_ptr__3_0_t",""],["__itt_relation_add_ptr__3_0_t",""],["__itt_relation_add_to_current_ex_ptr__3_0_t",""],["__itt_relation_add_to_current_ptr__3_0_t",""],["__itt_resume_ptr__3_0_t",""],["__itt_scope","@brief Describes the scope of an event object in the trace."],["__itt_section_info",""],["__itt_section_type","@cond exclude_from_documentation"],["__itt_set_track_ptr__3_0_t",""],["__itt_string_handle",""],["__itt_string_handle_create_ptr__3_0_t",""],["__itt_suppress_clear_range_ptr__3_0_t",""],["__itt_suppress_mark_range_ptr__3_0_t",""],["__itt_suppress_mode","@enum __itt_model_disable @brief Enumerator for the disable methods"],["__itt_suppress_pop_ptr__3_0_t",""],["__itt_suppress_push_ptr__3_0_t",""],["__itt_sync_acquired_ptr__3_0_t",""],["__itt_sync_cancel_ptr__3_0_t",""],["__itt_sync_create_ptr__3_0_t",""],["__itt_sync_destroy_ptr__3_0_t",""],["__itt_sync_prepare_ptr__3_0_t",""],["__itt_sync_releasing_ptr__3_0_t",""],["__itt_sync_rename_ptr__3_0_t",""],["__itt_task_begin_ex_ptr__3_0_t",""],["__itt_task_begin_fn_ex_ptr__3_0_t",""],["__itt_task_begin_fn_ptr__3_0_t",""],["__itt_task_begin_overlapped_ptr__3_0_t",""],["__itt_task_begin_ptr__3_0_t",""],["__itt_task_end_ex_ptr__3_0_t",""],["__itt_task_end_overlapped_ptr__3_0_t",""],["__itt_task_end_ptr__3_0_t",""],["__itt_task_group_ptr__3_0_t",""],["__itt_thread_ignore_ptr__3_0_t",""],["__itt_thread_set_name_ptr__3_0_t",""],["__itt_timestamp","@cond exclude_from_documentation"],["__itt_track",""],["__itt_track_create_ptr__3_0_t",""],["__itt_track_group",""],["__itt_track_group_create_ptr__3_0_t",""],["_iJIT_CodeArchitecture","@brief Enumerator for the code architecture."],["_iJIT_IsProfilingActiveFlags","@brief Enumerator for the agent’s mode"],["_iJIT_SegmentType","@cond exclude_from_documentation */ @brief Description of a segment type @details Use the segment type to specify a type of data supplied with the iJVM_EVENT_TYPE_METHOD_UPDATE_V2 event to be applied to a certain code trace."],["iJIT_Method_Inline_Load","@brief Description of an inline JIT-compiled method @details When you use the_iJIT_Method_Inline_Load structure to describe the JIT compiled method, use iJVM_EVENT_TYPE_METHOD_INLINE_LOAD_FINISHED as an event type to report it."],["iJIT_Method_Load","@brief Description of a JIT-compiled method @details When you use the iJIT_Method_Load structure to describe the JIT compiled method, use iJVM_EVENT_TYPE_METHOD_LOAD_FINISHED as an event type to report it."],["iJIT_Method_Load_V2","@brief Description of a JIT-compiled method @details When you use the iJIT_Method_Load_V2 structure to describe the JIT compiled method, use iJVM_EVENT_TYPE_METHOD_LOAD_FINISHED_V2 as an event type to report it."],["iJIT_Method_Load_V3","@brief Description of a JIT-compiled method @details The iJIT_Method_Load_V3 structure is the same as iJIT_Method_Load_V2 with a newly introduced ‘arch’ field that specifies architecture of the code region. When you use the iJIT_Method_Load_V3 structure to describe the JIT compiled method, use iJVM_EVENT_TYPE_METHOD_LOAD_FINISHED_V3 as an event type to report it."],["iJIT_Method_Update","@brief Description of a dynamic update of the content within JIT-compiled method @details The JIT engine may generate the methods that are updated at runtime partially by mixed (data + executable code) content. When you use the iJIT_Method_Update structure to describe the update of the content within a JIT-compiled method, use iJVM_EVENT_TYPE_METHOD_UPDATE_V2 as an event type to report it."],["iJIT_jvm_event","@brief Enumerator for the types of notifications"],["pLineNumberInfo","@brief Description of a single entry in the line number information of a code region. @details A table of line number entries gives information about how the reported code region is mapped to source file. Intel(R) VTune(TM) Amplifier uses line number information to attribute the samples (virtual address) to a line number. \\n It is acceptable to report different code addresses for the same source line: @code Offset LineNumber 1       2 12      4 15      2 18      1 21      30"],["piJIT_Method_Inline_Load","@brief Description of an inline JIT-compiled method @details When you use the_iJIT_Method_Inline_Load structure to describe the JIT compiled method, use iJVM_EVENT_TYPE_METHOD_INLINE_LOAD_FINISHED as an event type to report it."],["piJIT_Method_Load","@brief Description of a JIT-compiled method @details When you use the iJIT_Method_Load structure to describe the JIT compiled method, use iJVM_EVENT_TYPE_METHOD_LOAD_FINISHED as an event type to report it."],["piJIT_Method_Load_V2","@brief Description of a JIT-compiled method @details When you use the iJIT_Method_Load_V2 structure to describe the JIT compiled method, use iJVM_EVENT_TYPE_METHOD_LOAD_FINISHED_V2 as an event type to report it."],["piJIT_Method_Load_V3","@brief Description of a JIT-compiled method @details The iJIT_Method_Load_V3 structure is the same as iJIT_Method_Load_V2 with a newly introduced ‘arch’ field that specifies architecture of the code region. When you use the iJIT_Method_Load_V3 structure to describe the JIT compiled method, use iJVM_EVENT_TYPE_METHOD_LOAD_FINISHED_V3 as an event type to report it."],["piJIT_Method_Update","@brief Description of a dynamic update of the content within JIT-compiled method @details The JIT engine may generate the methods that are updated at runtime partially by mixed (data + executable code) content. When you use the iJIT_Method_Update structure to describe the update of the content within a JIT-compiled method, use iJVM_EVENT_TYPE_METHOD_UPDATE_V2 as an event type to report it."],["size_t",""]]});