-- assets.lua for PaperMC software family
function get_assets(version)
    local project = "paper"
    local url = "https://fill.papermc.io/v3/projects/" .. project .. "/versions/" .. version .. "/builds"
    local resp = craft.http.get(url)
    if resp.ok then
        local builds = craft.json.decode(resp.body)
        if builds and #builds > 0 then
            local latest = builds[#builds]
            if latest.downloads and latest.downloads.application then
                return {
                    {
                        filename = "paper.jar",
                        url = latest.downloads.application.url,
                        sha256 = latest.downloads.application.sha256,
                        is_archive = false
                    }
                }
            end
        end
    end
    -- Fallback
    return {
        {
            filename = "paper.jar",
            url = "https://api.papermc.io/v2/projects/" .. project .. "/versions/" .. version .. "/builds/latest/download",
            is_archive = false
        }
    }
end
