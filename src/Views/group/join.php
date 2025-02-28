<div class="row justify-content-center">
    <div class="col-md-8 col-lg-6">
        <div class="card">
            <div class="card-header">
                <h3 class="card-title">Join a Group</h3>
            </div>
            <div class="card-body">
                <p class="card-text">Enter the invitation code you received to join a Secret Santa group.</p>
                
                <form action="/groups/join" method="post">
                    <div class="mb-3">
                        <label for="invitation_code" class="form-label">Invitation Code <span class="text-danger">*</span></label>
                        <input type="text" class="form-control" id="invitation_code" name="invitation_code" required 
                               value="<?= htmlspecialchars($session->getOldInput('invitation_code', $code ?? '')) ?>"
                               placeholder="Enter the code (e.g., ABC12345)">
                        <div class="form-text">The invitation code is case-sensitive.</div>
                    </div>
                    
                    <div class="d-grid gap-2">
                        <button type="submit" class="btn btn-primary">Join Group</button>
                        <a href="/groups" class="btn btn-outline-secondary">Cancel</a>
                    </div>
                </form>
            </div>
        </div>
    </div>
</div>