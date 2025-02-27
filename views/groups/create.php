<div class="row">
    <div class="col-lg-8 mx-auto">
        <div class="card">
            <div class="card-header">
                <h4 class="mb-0">Create a New Secret Santa Group</h4>
            </div>
            <div class="card-body">
                <form action="/groups" method="post">
                    <input type="hidden" name="csrf_token" value="<?= $csrf ?>">
                    
                    <div class="mb-3">
                        <label for="name" class="form-label">Group Name <span class="text-danger">*</span></label>
                        <input type="text" class="form-control <?= isset($errors['name']) ? 'is-invalid' : '' ?>" 
                            id="name" name="name" value="<?= $name ?? '' ?>" required>
                        <?php if (isset($errors['name'])): ?>
                            <div class="invalid-feedback"><?= $errors['name'] ?></div>
                        <?php endif; ?>
                        <div class="form-text">Give your Secret Santa group a memorable name</div>
                    </div>
                    
                    <div class="mb-3">
                        <label for="description" class="form-label">Description</label>
                        <textarea class="form-control <?= isset($errors['description']) ? 'is-invalid' : '' ?>" 
                            id="description" name="description" rows="3"><?= $description ?? '' ?></textarea>
                        <?php if (isset($errors['description'])): ?>
                            <div class="invalid-feedback"><?= $errors['description'] ?></div>
                        <?php endif; ?>
                        <div class="form-text">Optional details about your Secret Santa group</div>
                    </div>
                    
                    <div class="row">
                        <div class="col-md-6">
                            <div class="mb-3">
                                <label for="join_deadline" class="form-label">Join Deadline</label>
                                <input type="date" class="form-control <?= isset($errors['join_deadline']) ? 'is-invalid' : '' ?>" 
                                    id="join_deadline" name="join_deadline" value="<?= $join_deadline ?? '' ?>">
                                <?php if (isset($errors['join_deadline'])): ?>
                                    <div class="invalid-feedback"><?= $errors['join_deadline'] ?></div>
                                <?php endif; ?>
                                <div class="form-text">Last day for members to join (optional)</div>
                            </div>
                        </div>
                        <div class="col-md-6">
                            <div class="mb-3">
                                <label for="draw_date" class="form-label">Draw Date</label>
                                <input type="date" class="form-control <?= isset($errors['draw_date']) ? 'is-invalid' : '' ?>" 
                                    id="draw_date" name="draw_date" value="<?= $draw_date ?? '' ?>">
                                <?php if (isset($errors['draw_date'])): ?>
                                    <div class="invalid-feedback"><?= $errors['draw_date'] ?></div>
                                <?php endif; ?>
                                <div class="form-text">When the draw should happen (optional)</div>
                            </div>
                        </div>
                    </div>
                    
                    <div class="mb-4">
                        <label class="form-label">Wishlist Visibility</label>
                        <div class="form-check">
                            <input class="form-check-input" type="radio" name="wishlist_visibility" id="visibility_all" 
                                value="all" <?= ($wishlist_visibility ?? 'all') == 'all' ? 'checked' : '' ?>>
                            <label class="form-check-label" for="visibility_all">
                                All group members can see each other's wishlists
                            </label>
                        </div>
                        <div class="form-check">
                            <input class="form-check-input" type="radio" name="wishlist_visibility" id="visibility_santa" 
                                value="santa_only" <?= ($wishlist_visibility ?? '') == 'santa_only' ? 'checked' : '' ?>>
                            <label class="form-check-label" for="visibility_santa">
                                Only Secret Santas can see their recipient's wishlist
                            </label>
                        </div>
                    </div>
                    
                    <div class="text-center">
                        <button type="submit" class="btn btn-primary px-5">Create Group</button>
                    </div>
                </form>
            </div>
        </div>
    </div>
</div>
