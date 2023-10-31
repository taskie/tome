# Actions

* `ichno::actions::pre_process`
  * `ichno::db::actions::create_workspace_if_needed`
    * workspaces
  * `ichno::db::actions::create_group_if_needed`
    * groups
* `ichno::actions::update_file_stat`
  * `ichno::db::actions::update_stat_with_paths_if_needed`
    * if stat is enabled:
      * `ichno::db::actions::create_footprint_if_needed`
        * footprints
      * `ichno::db::actions::update_stat_with_footprint_if_needed`
        * `ichno::db::actions::create_history_with_footprint_if_needed`
          * histories
        * stats
    * else:
      * `ichno::db::actions::update_disabled_stat_if_needed`
* `ichno::actions::post_process`
  * `ichno::db::actions::update_meta_group_stat`
    * `ichno::db::actions::create_meta_group_if_needed`
      * `ichno::db::actions::create_group_if_needed`
        * groups
    * `ichno::db::actions::update_stat_with_present_paths_if_needed`
      * `ichno::db::actions::update_stat_with_paths_if_needed`
        * `ichno::db::actions::update_stat_with_paths_if_needed`
          * ...
    * groups
