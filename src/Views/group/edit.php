<div class="row justify-content-center">
    <div class="col-md-8 col-lg-6">
        <div class="card">
            <div class="card-header">
                <h3 class="card-title">Edit Group</h3>
            </div>
            <div class="card-body">
                <form action="/groups/<?= $group->getId() ?>/edit" method="post">
                    <div class="mb-3">
                        <label for="name" class="form-label">Group Name <span class="text-danger">*</span></label>
                        <input type="text" class="form-control" id="name" name="name" required 
                               value="<?= htmlspecialchars($session->getOldInput('name', $group->getName())) ?>">
                        <div class="form-text">Choose a name for your Secret Santa group.</div>
                    </div>
                    
                    <div class="mb-3">
                        <label for="description" class="form-label">Description</label>
                        <textarea class="form-control" id="description" name="description" rows="3"><?= htmlspecialchars($session->getOldInput('description', $group->getDescription() ?? '')) ?></textarea>
                        <div class="form-text">Optional description for your group.</div>
                    </div>
                    
                    <div class="mb-3">
                        <label for="registration_deadline" class="form-label">Registration Deadline</label>
                        <input type="date" class="form-control" id="registration_deadline" name="registration_deadline" 
                               value="<?= htmlspecialchars($session->getOldInput('registration_deadline', $group->getRegistrationDeadline() ? date('Y-m-d', strtotime($group->getRegistrationDeadline())) : '')) ?>">
                        <div class="form-text">Last day for participants to join the group. Leave blank for no deadline.</div>
                    </div>
                    
                    <div class="mb-3">
                        <label for="draw_date" class="form-label">Draw Date</label>
                        <input type="date" class="form-control" id="draw_date" name="draw_date" 
                               value="<?= htmlspecialchars($session->getOldInput('draw_date', $group->getDrawDate() ? date('Y-m-d', strtotime($group->getDrawDate())) : '')) ?>">
                        <div class="form-text">When the Secret Santa draw will take place. Leave blank to decide later.</div>
                    </div>
                    
                    <div class="d-grid gap-2">
                        <button type="submit" class="btn btn-primary">Update Group</button>
                        <a href="/groups/<?= $group->getId() ?>" class="btn btn-outline-secondary">Cancel</a>
                    </div>
                </form>
            </div>
        </div>
    </div>
</div>