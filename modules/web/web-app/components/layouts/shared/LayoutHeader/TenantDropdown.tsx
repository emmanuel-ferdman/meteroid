import {
  Button,
  Command,
  CommandEmpty,
  CommandItem,
  Popover,
  PopoverContent,
  PopoverTrigger,
} from '@md/ui'
import { CommandList } from 'cmdk'
import { ChevronsUpDownIcon, PlusIcon } from 'lucide-react'
import { useEffect, useState } from 'react'
import { Link } from 'react-router-dom'

import { useOrganizationSlug } from '@/hooks/useOrganization'
import { useTenant } from '@/hooks/useTenant'
import { useQuery } from '@/lib/connectrpc'
import { listTenants } from '@/rpc/api/tenants/v1/tenants-TenantsService_connectquery'

export const TenantDropdown = () => {
  const tenants = useQuery(listTenants).data?.tenants ?? []

  const { tenant } = useTenant()
  const org = useOrganizationSlug()

  const [open, setOpen] = useState(false)

  useEffect(() => {
    setOpen(false)
  }, [tenant])

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger className="h-9 px-4 py-2 rounded-full border border-border bg-background dark:border-0 dark:bg-secondary hover:bg-accent dark:hover:bg-accent ">
        <div className="flex flex-row space-x-2 items-center ">
          <span className="text-xs text-muted-foreground">Tenant: </span>
          <span className=" rounded-full p-1 bg-cyan-600" />
          <span>{tenant?.name}</span>
          <ChevronsUpDownIcon size="10" />
        </div>
      </PopoverTrigger>
      <PopoverContent className="w-[200px] p-0">
        <Command>
          <CommandEmpty>No tenant found.</CommandEmpty>
          <CommandList>
            {tenants
              .sort((a, b) => a.name.localeCompare(b.name))
              .map(tenant => (
                <Link key={tenant.id} to={`/${org}/${tenant.slug}`}>
                  <CommandItem key={tenant.id}>{tenant.name}</CommandItem>
                </Link>
              ))}
          </CommandList>
          <CommandItem>
            <Link to={`/${org}/tenants/new`} className="w-full">
              <Button size="content" variant="ghost" hasIcon>
                <PlusIcon size="12" /> New tenant
              </Button>
            </Link>
          </CommandItem>
        </Command>
      </PopoverContent>
    </Popover>
  )
}
